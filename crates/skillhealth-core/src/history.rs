use crate::model::Skill;
use chrono::{DateTime, TimeZone, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Typed invocations of one command from the runtime's prompt history
/// (`<config>/history.jsonl`). SECOND usage source, surface-only: it never
/// feeds heat — the two sources overlap, merging would double-count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct HistoryStats {
    pub count: u64,
    pub last_used: Option<DateTime<Utc>>,
}

/// Parse history.jsonl: one JSON object per typed prompt with `display`
/// (the text), `project` (session cwd) and `timestamp` (epoch ms). Keys of
/// the result are the typed command without the slash. Missing/unreadable
/// file → empty (the feature is silently absent).
pub fn scan_history(path: &Path, lens_root: Option<&Path>) -> HashMap<String, HistoryStats> {
    let mut out: HashMap<String, HistoryStats> = HashMap::new();
    let Ok(raw) = std::fs::read_to_string(path) else {
        return out;
    };
    let lens_root = lens_root.map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
    for line in raw.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(display) = v.get("display").and_then(|d| d.as_str()) else {
            continue;
        };
        let Some(rest) = display.strip_prefix('/') else {
            continue;
        };
        let Some(cmd) = rest.split_whitespace().next() else {
            continue;
        };
        if let Some(root) = &lens_root {
            // No project field → can't attribute → global-only.
            let Some(project) = v.get("project").and_then(|p| p.as_str()) else {
                continue;
            };
            let project = std::fs::canonicalize(project).unwrap_or_else(|_| PathBuf::from(project));
            if !project.starts_with(root) {
                continue;
            }
        }
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_i64())
            .and_then(|ms| Utc.timestamp_millis_opt(ms).single());
        let e = out.entry(cmd.to_string()).or_default();
        e.count += 1;
        if ts > e.last_used {
            e.last_used = ts;
        }
    }
    out
}

/// Attribute typed commands to installed skills, keyed by skill NAME.
/// Always match the full name; additionally the bare dir_name for
/// plugin-prefixed names when exactly ONE installed skill owns that
/// dir_name (typed commands omit the prefix). Ambiguous bare names are
/// dropped, not guessed. Builtins (`/resume`, …) match no skill and fall out.
pub fn match_to_skills(
    skills: &[Skill],
    raw: &HashMap<String, HistoryStats>,
) -> HashMap<String, HistoryStats> {
    let mut owners: HashMap<&str, usize> = HashMap::new();
    for s in skills {
        *owners.entry(s.dir_name.as_str()).or_default() += 1;
    }
    let mut out = HashMap::new();
    for s in skills {
        let mut acc = HistoryStats::default();
        if let Some(h) = raw.get(&s.name) {
            fold(&mut acc, h);
        }
        if s.name != s.dir_name
            && owners.get(s.dir_name.as_str()) == Some(&1)
            && let Some(h) = raw.get(&s.dir_name)
        {
            fold(&mut acc, h);
        }
        if acc.count > 0 {
            out.insert(s.name.clone(), acc);
        }
    }
    out
}

fn fold(acc: &mut HistoryStats, h: &HistoryStats) {
    acc.count += h.count;
    if h.last_used > acc.last_used {
        acc.last_used = h.last_used;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SkillSource;
    use std::io::Write;

    fn write_history(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        f
    }

    fn skill(name: &str, dir: &str) -> Skill {
        Skill {
            name: name.into(),
            dir_name: dir.into(),
            source: if name.contains(':') {
                SkillSource::Plugin(name.split(':').next().unwrap().into())
            } else {
                SkillSource::User
            },
            path: PathBuf::from(format!("/s/{dir}/SKILL.md")),
            description: Some("d".into()),
            frontmatter_ok: true,
            body: String::new(),
            est_tokens: 10,
            always_on_tokens: 5,
            disabled: false,
        }
    }

    #[test]
    fn counts_typed_slash_commands_with_latest_timestamp() {
        let f = write_history(&[
            r#"{"display":"/cfo check","timestamp":1777490357212,"project":"/Users/me/x"}"#,
            r#"{"display":"/cfo","timestamp":1777490457212,"project":"/Users/me/y"}"#,
            r#"{"display":"plain prompt","timestamp":1777490357212,"project":"/Users/me/x"}"#,
            r#"not json"#,
        ]);
        let raw = scan_history(f.path(), None);
        assert_eq!(raw["cfo"].count, 2);
        assert_eq!(
            raw["cfo"].last_used,
            Utc.timestamp_millis_opt(1777490457212).single()
        );
        assert_eq!(raw.len(), 1);
    }

    #[test]
    fn lens_filters_by_project_field() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let inside = root.join("repo");
        std::fs::create_dir_all(&inside).unwrap();
        // Build with serde_json so the path is escaped correctly on every
        // platform: a raw `{}` interpolation of a Windows `C:\…` path yields
        // invalid JSON escapes, the line is dropped, and the lens under-counts.
        let in_line = serde_json::json!({
            "display": "/cfo",
            "timestamp": 1777490357212i64,
            "project": inside.to_str().unwrap(),
        })
        .to_string();
        let f = write_history(&[
            in_line.as_str(),
            r#"{"display":"/cfo","timestamp":1777490357212,"project":"/elsewhere"}"#,
            r#"{"display":"/cfo","timestamp":1777490357212}"#,
        ]);
        let raw = scan_history(f.path(), Some(&inside));
        assert_eq!(raw["cfo"].count, 1); // only the inside-root line matches the lens
    }

    #[test]
    fn missing_file_is_empty() {
        assert!(scan_history(Path::new("/nope/history.jsonl"), None).is_empty());
    }

    #[test]
    fn match_full_name_and_unambiguous_bare_plugin_name() {
        let skills = vec![
            skill("cfo", "cfo"),
            skill("superpowers:writing-plans", "writing-plans"),
        ];
        let mut raw: HashMap<String, HistoryStats> = HashMap::new();
        raw.insert(
            "cfo".into(),
            HistoryStats {
                count: 3,
                last_used: None,
            },
        );
        raw.insert(
            "writing-plans".into(),
            HistoryStats {
                count: 2,
                last_used: None,
            },
        );
        raw.insert(
            "superpowers:writing-plans".into(),
            HistoryStats {
                count: 1,
                last_used: None,
            },
        );
        raw.insert(
            "resume".into(),
            HistoryStats {
                count: 9,
                last_used: None,
            },
        ); // builtin: matches nothing
        let m = match_to_skills(&skills, &raw);
        assert_eq!(m["cfo"].count, 3);
        // bare (2, unambiguous owner) + prefixed (1)
        assert_eq!(m["superpowers:writing-plans"].count, 3);
        assert!(!m.contains_key("resume"));
    }

    #[test]
    fn ambiguous_bare_names_are_dropped_not_guessed() {
        let skills = vec![skill("aaa:deploy", "deploy"), skill("bbb:deploy", "deploy")];
        let mut raw: HashMap<String, HistoryStats> = HashMap::new();
        raw.insert(
            "deploy".into(),
            HistoryStats {
                count: 5,
                last_used: None,
            },
        );
        let m = match_to_skills(&skills, &raw);
        assert!(m.is_empty(), "two owners → bare key dropped; got {m:?}");
    }
}
