use crate::cache::{CACHE_VERSION, CachedFile, UsageCache};
use crate::model::{UsageStats, iso_week_key};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use walkdir::WalkDir;

pub struct FileScan {
    pub stats: HashMap<String, UsageStats>,
    /// First `cwd` value seen in the transcript — the session's project
    /// provenance. Not guaranteed on line 1 (summary/meta lines lack it).
    pub cwd: Option<String>,
}

pub fn scan_file(path: &Path) -> io::Result<FileScan> {
    let reader = BufReader::new(File::open(path)?);
    let mut stats: HashMap<String, UsageStats> = HashMap::new();
    let mut cwd: Option<String> = None;
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if cwd.is_none()
            && line.contains(r#""cwd":"#)
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
            && let Some(c) = v.get("cwd").and_then(|c| c.as_str())
        {
            cwd = Some(c.to_string());
        }
        let has_skill_tool = line.contains(r#""name":"Skill""#);
        let has_command = line.contains("<command-name>");
        if !has_skill_tool && !has_command {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc));
        if has_skill_tool
            && let Some(content) = v.pointer("/message/content").and_then(|c| c.as_array())
        {
            for item in content {
                if item.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                    && item.get("name").and_then(|n| n.as_str()) == Some("Skill")
                    && let Some(name) = item.pointer("/input/skill").and_then(|s| s.as_str())
                {
                    bump(&mut stats, name, ts);
                }
            }
        }
        if has_command && let Some(name) = extract_command_name(&line) {
            bump(&mut stats, &name, ts);
        }
    }
    Ok(FileScan { stats, cwd })
}

fn extract_command_name(line: &str) -> Option<String> {
    let start = line.find("<command-name>")? + "<command-name>".len();
    let end = line[start..].find("</command-name>")? + start;
    let raw = line[start..end].trim();
    let name = raw.strip_prefix('/').unwrap_or(raw);
    let name = name.split_whitespace().next()?;
    (!name.is_empty()).then(|| name.to_string())
}

fn bump(stats: &mut HashMap<String, UsageStats>, name: &str, ts: Option<DateTime<Utc>>) {
    let e = stats.entry(name.to_string()).or_default();
    e.count += 1;
    if let Some(t) = ts {
        *e.week_counts.entry(iso_week_key(t)).or_insert(0) += 1;
    }
    if ts > e.last_used {
        e.last_used = ts;
    }
}

pub fn merge(into: &mut HashMap<String, UsageStats>, from: &HashMap<String, UsageStats>) {
    for (name, s) in from {
        let e = into.entry(name.clone()).or_default();
        e.count += s.count;
        if s.last_used > e.last_used {
            e.last_used = s.last_used;
        }
        for (week, n) in &s.week_counts {
            *e.week_counts.entry(week.clone()).or_insert(0) += n;
        }
    }
}

pub struct UsageScan {
    pub per_skill: HashMap<String, UsageStats>,
    pub files_total: usize,
    pub files_rescanned: usize,
}

/// `on_progress(done, total)` is called from worker threads after each rescanned file.
/// `lens_root`: when `Some(project_dir)`, only transcripts whose recorded session
/// `cwd` sits under that directory contribute to `per_skill`. `files_total` always
/// reflects the global file count — the lens narrows heat, not file discovery.
pub fn scan_usage(
    projects_dir: &Path,
    cache: &mut UsageCache,
    lens_root: Option<&Path>,
    on_progress: &(dyn Fn(usize, usize) + Sync),
) -> UsageScan {
    let mut files: Vec<(PathBuf, u128, u64)> = Vec::new();
    if projects_dir.is_dir() {
        for entry in WalkDir::new(projects_dir)
            .max_depth(3)
            .into_iter()
            .flatten()
        {
            let p = entry.path();
            if !entry.file_type().is_file() || p.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Ok(md) = entry.metadata() else { continue };
            let mtime = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            files.push((p.to_path_buf(), mtime, md.len()));
        }
    }

    let (cached, to_scan): (Vec<_>, Vec<_>) = files.iter().partition(|(p, mtime, size)| {
        cache
            .files
            .get(p.to_string_lossy().as_ref())
            .is_some_and(|c| c.mtime_ms == *mtime && c.size == *size)
    });

    let total = to_scan.len();
    let done = AtomicUsize::new(0);
    let scanned: Vec<(PathBuf, u128, u64, FileScan)> = to_scan
        .par_iter()
        .map(|(p, mtime, size)| {
            let scan = scan_file(p).unwrap_or(FileScan {
                stats: HashMap::new(),
                cwd: None,
            });
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            on_progress(n, total);
            (p.clone(), *mtime, *size, scan)
        })
        .collect();

    // rebuild cache: only files that still exist
    let mut new_files: HashMap<String, CachedFile> = HashMap::new();
    for (p, _, _) in &cached {
        let key = p.to_string_lossy().into_owned();
        if let Some(entry) = cache.files.get(&key) {
            new_files.insert(key, entry.clone());
        }
    }
    for (p, mtime, size, scan) in scanned {
        new_files.insert(
            p.to_string_lossy().into_owned(),
            CachedFile {
                mtime_ms: mtime,
                size,
                stats: scan.stats,
                cwd: scan.cwd,
            },
        );
    }
    cache.files = new_files;
    cache.version = CACHE_VERSION;

    let lens_root = lens_root.map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
    let mut per_skill = HashMap::new();
    for entry in cache.files.values() {
        if let Some(root) = &lens_root {
            // No cwd recorded → session can't be attributed → global-only.
            let Some(cwd) = entry.cwd.as_deref() else {
                continue;
            };
            let cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd));
            if !cwd.starts_with(root) {
                continue;
            }
        }
        merge(&mut per_skill, &entry.stats);
    }
    UsageScan {
        per_skill,
        files_total: files.len(),
        files_rescanned: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::io::Write;

    fn write_jsonl(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        f
    }

    #[test]
    fn counts_skill_tool_invocations_with_latest_timestamp() {
        let f = write_jsonl(&[
            r#"{"type":"assistant","timestamp":"2026-06-01T10:00:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Skill","input":{"skill":"cfo","args":"check"}}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-06-08T09:00:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Skill","input":{"skill":"cfo"}}]}}"#,
        ]);
        let scan = scan_file(f.path()).unwrap();
        let cfo = &scan.stats["cfo"];
        assert_eq!(cfo.count, 2);
        assert_eq!(
            cfo.last_used,
            Some(Utc.with_ymd_and_hms(2026, 6, 8, 9, 0, 0).unwrap())
        );
    }

    #[test]
    fn counts_command_name_invocations() {
        let f = write_jsonl(&[
            r#"{"type":"user","timestamp":"2026-06-05T08:00:00.000Z","message":{"role":"user","content":"<command-name>/commit-msg</command-name> for the staged diff"}}"#,
        ]);
        let scan = scan_file(f.path()).unwrap();
        assert_eq!(scan.stats["commit-msg"].count, 1);
    }

    #[test]
    fn namespaced_command_names_are_kept() {
        let f = write_jsonl(&[
            r#"{"type":"user","timestamp":"2026-06-05T08:00:00.000Z","message":{"role":"user","content":"<command-name>/superpowers:writing-plans</command-name>"}}"#,
        ]);
        let scan = scan_file(f.path()).unwrap();
        assert!(scan.stats.contains_key("superpowers:writing-plans"));
    }

    #[test]
    fn skips_irrelevant_and_malformed_lines() {
        let f = write_jsonl(&[
            r#"{"type":"user","timestamp":"2026-06-05T08:00:00.000Z","message":{"role":"user","content":"just chatting about Skills in general"}}"#,
            r#"not json at all {{{"#,
            r#"{"type":"assistant","timestamp":"2026-06-05T08:01:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
        ]);
        let scan = scan_file(f.path()).unwrap();
        assert!(scan.stats.is_empty());
    }

    #[test]
    fn scan_file_captures_first_cwd() {
        let f = write_jsonl(&[
            r#"{"type":"summary","leafUuid":"x"}"#,
            r#"{"type":"user","cwd":"/Users/me/dev/proj","timestamp":"2026-06-05T08:00:00.000Z","message":{"role":"user","content":"<command-name>/cfo</command-name>"}}"#,
            r#"{"type":"user","cwd":"/Users/me/elsewhere","timestamp":"2026-06-05T09:00:00.000Z","message":{"role":"user","content":"hi"}}"#,
        ]);
        let scan = scan_file(f.path()).unwrap();
        assert_eq!(scan.cwd.as_deref(), Some("/Users/me/dev/proj"));
        assert_eq!(scan.stats["cfo"].count, 1);
    }

    #[test]
    fn scan_file_without_cwd_yields_none() {
        let f = write_jsonl(&[
            r#"{"type":"assistant","timestamp":"2026-06-01T10:00:00.000Z","message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"cfo"}}]}}"#,
        ]);
        let scan = scan_file(f.path()).unwrap();
        assert!(scan.cwd.is_none());
        assert_eq!(scan.stats["cfo"].count, 1);
    }

    #[test]
    fn bump_fills_week_counts_per_iso_week() {
        let f = write_jsonl(&[
            r#"{"type":"assistant","timestamp":"2026-06-01T10:00:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Skill","input":{"skill":"cfo"}}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-06-08T09:00:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Skill","input":{"skill":"cfo"}}]}}"#,
            r#"{"type":"assistant","timestamp":"2026-06-08T11:00:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t3","name":"Skill","input":{"skill":"cfo"}}]}}"#,
        ]);
        let scan = scan_file(f.path()).unwrap();
        // 2026-06-01 is in ISO week 23, 2026-06-08 in week 24
        assert_eq!(scan.stats["cfo"].week_counts["2026-W23"], 1);
        assert_eq!(scan.stats["cfo"].week_counts["2026-W24"], 2);
    }

    #[test]
    fn merge_sums_week_counts() {
        let mut a: HashMap<String, UsageStats> = HashMap::new();
        let mut b: HashMap<String, UsageStats> = HashMap::new();
        let mut s1 = UsageStats {
            count: 1,
            ..Default::default()
        };
        s1.week_counts.insert("2026-W23".into(), 1);
        let mut s2 = UsageStats {
            count: 2,
            ..Default::default()
        };
        s2.week_counts.insert("2026-W23".into(), 1);
        s2.week_counts.insert("2026-W24".into(), 1);
        a.insert("cfo".into(), s1);
        b.insert("cfo".into(), s2);
        merge(&mut a, &b);
        assert_eq!(a["cfo"].count, 3);
        assert_eq!(a["cfo"].week_counts["2026-W23"], 2);
        assert_eq!(a["cfo"].week_counts["2026-W24"], 1);
    }

    #[test]
    fn lens_root_filters_aggregation_by_file_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        // canonicalize: macOS tempdirs are symlinked, prod code canonicalizes
        let root = tmp.path().canonicalize().unwrap();
        let projects = root.join("projects").join("p");
        std::fs::create_dir_all(&projects).unwrap();
        let proj_dir = root.join("dev").join("repo");
        std::fs::create_dir_all(&proj_dir).unwrap();
        // serde_json escapes the path correctly on every platform: a raw
        // interpolation of a Windows `C:\…` cwd yields invalid JSON escapes,
        // the line is dropped, and heat is silently under-counted on Windows.
        let in_line = serde_json::json!({
            "type": "user",
            "cwd": proj_dir.to_str().unwrap(),
            "timestamp": "2026-06-05T08:00:00.000Z",
            "message": {"role": "user", "content": "<command-name>/cfo</command-name>"},
        })
        .to_string();
        std::fs::write(projects.join("in.jsonl"), in_line).unwrap();
        std::fs::write(projects.join("out.jsonl"),
            r#"{"type":"user","cwd":"/somewhere/else","timestamp":"2026-06-05T08:00:00.000Z","message":{"role":"user","content":"<command-name>/cfo</command-name>"}}"#,
        ).unwrap();
        std::fs::write(projects.join("nocwd.jsonl"),
            r#"{"type":"user","timestamp":"2026-06-05T08:00:00.000Z","message":{"role":"user","content":"<command-name>/cfo</command-name>"}}"#,
        ).unwrap();

        let mut cache = crate::cache::UsageCache::default();
        // global lens: all three files count
        let g = scan_usage(&root.join("projects"), &mut cache, None, &|_, _| {});
        assert_eq!(g.per_skill["cfo"].count, 3);
        // project lens: only the file whose cwd is under proj_dir
        let mut cache2 = crate::cache::UsageCache::default();
        let p = scan_usage(
            &root.join("projects"),
            &mut cache2,
            Some(&proj_dir),
            &|_, _| {},
        );
        assert_eq!(p.per_skill["cfo"].count, 1);
        // files_total stays global — the lens narrows heat, not discovery of files
        assert_eq!(p.files_total, 3);
    }

    #[test]
    fn scan_usage_uses_cache_and_detects_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects").join("proj-a");
        std::fs::create_dir_all(&projects).unwrap();
        let f1 = projects.join("s1.jsonl");
        std::fs::write(&f1,
            r#"{"type":"assistant","timestamp":"2026-06-01T10:00:00.000Z","message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"cfo"}}]}}"#,
        ).unwrap();

        let mut cache = crate::cache::UsageCache::default();
        let scan1 = scan_usage(
            tmp.path().join("projects").as_path(),
            &mut cache,
            None,
            &|_, _| {},
        );
        assert_eq!(scan1.per_skill["cfo"].count, 1);
        assert_eq!(scan1.files_rescanned, 1);

        // second run, nothing changed → served from cache
        let scan2 = scan_usage(
            tmp.path().join("projects").as_path(),
            &mut cache,
            None,
            &|_, _| {},
        );
        assert_eq!(scan2.files_rescanned, 0);
        assert_eq!(scan2.per_skill["cfo"].count, 1);

        // append a line (size changes) → rescanned
        let mut content = std::fs::read_to_string(&f1).unwrap();
        content.push_str("\n{\"type\":\"assistant\",\"timestamp\":\"2026-06-09T10:00:00.000Z\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Skill\",\"input\":{\"skill\":\"cfo\"}}]}}");
        std::fs::write(&f1, content).unwrap();
        let scan3 = scan_usage(
            tmp.path().join("projects").as_path(),
            &mut cache,
            None,
            &|_, _| {},
        );
        assert_eq!(scan3.files_rescanned, 1);
        assert_eq!(scan3.per_skill["cfo"].count, 2);

        // delete the file → cache entry dropped
        std::fs::remove_file(&f1).unwrap();
        let scan4 = scan_usage(
            tmp.path().join("projects").as_path(),
            &mut cache,
            None,
            &|_, _| {},
        );
        assert!(scan4.per_skill.is_empty());
    }
}
