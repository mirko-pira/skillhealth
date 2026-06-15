use crate::claude_md::ClaudeMdInfo;
use crate::discover::Discovery;
use crate::model::{Severity, UsageStats};
use crate::usage::UsageScan;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

pub const CLAUDE_MD_TOKEN_BUDGET: usize = 5_000;
pub const SKILL_TOKEN_BUDGET: usize = 2_000;
pub const DEAD_SKILL_AGE_DAYS: i64 = 30;

const BUILTIN_COMMANDS: &[&str] = &[
    "clear",
    "help",
    "config",
    "model",
    "init",
    "compact",
    "login",
    "logout",
    "status",
    "review",
    "doctor",
    "cost",
    "memory",
    "vim",
    "permissions",
    "mcp",
    "agents",
    "resume",
    "export",
    "bug",
    "todos",
    "add-dir",
    "hooks",
    "ide",
    "statusline",
    "output-style",
    "terminal-setup",
    "install-github-app",
    "pr-comments",
    "release-notes",
    "fast",
    "schedule",
];

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub code: &'static str,
    pub title: String,
    pub why: String,
    /// Copy-pasteable shell line. Doctor NEVER applies it.
    pub fix: Option<String>,
    /// Skill name this finding is about, when applicable.
    pub skill: Option<String>,
}

pub fn run(
    d: &Discovery,
    scan: &UsageScan,
    claude_mds: &[ClaudeMdInfo],
    history: &HashMap<String, crate::history::HistoryStats>,
    now: DateTime<Utc>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    check_frontmatter(d, &mut out);
    check_shadowing(d, &mut out);
    check_debris(d, &mut out);
    check_dead(d, scan, now, &mut out);
    check_heavy_skills(d, &mut out);
    check_history_gap(d, scan, history, &mut out);
    for md in claude_mds {
        check_claude_md(md, d, &mut out);
    }
    out
}

fn check_frontmatter(d: &Discovery, out: &mut Vec<Finding>) {
    for s in &d.skills {
        if !s.frontmatter_ok {
            out.push(Finding {
                severity: Severity::Error,
                code: "E001",
                title: format!("broken frontmatter — {}", s.name),
                why: "SKILL.md has no parseable name/description; the runtime will never load it"
                    .into(),
                fix: Some(format!(
                    "$EDITOR \"{}\"  # add name + description frontmatter",
                    s.path.display()
                )),
                skill: Some(s.name.clone()),
            });
        } else if s.description.is_none() {
            out.push(Finding {
                severity: Severity::Warn,
                code: "W001",
                title: format!("missing description — {}", s.name),
                why: "without a description the model cannot decide when to trigger this skill"
                    .into(),
                fix: Some(format!(
                    "$EDITOR \"{}\"  # add a description: line",
                    s.path.display()
                )),
                skill: Some(s.name.clone()),
            });
        }
    }
}

fn check_shadowing(d: &Discovery, out: &mut Vec<Finding>) {
    let mut by_dir: HashMap<&str, usize> = HashMap::new();
    for s in &d.skills {
        *by_dir.entry(s.dir_name.as_str()).or_default() += 1;
    }
    for (dir, n) in by_dir {
        if n > 1 {
            out.push(Finding {
                severity: Severity::Warn,
                code: "W002",
                title: format!("shadowed skill — '{dir}' exists in {n} locations"),
                why: "project skills shadow user skills with the same name; only one will fire"
                    .into(),
                fix: None,
                skill: Some(dir.to_string()),
            });
        }
    }
}

fn check_debris(d: &Discovery, out: &mut Vec<Finding>) {
    for p in &d.debris {
        out.push(Finding {
            severity: Severity::Warn,
            code: "W003",
            title: format!("debris in skills dir — {}", p.display()),
            why: "directory has no SKILL.md; it is dead weight the runtime ignores".into(),
            fix: Some(format!(
                "# review contents, then: rm -r \"{}\"",
                p.display()
            )),
            skill: None,
        });
    }
}

fn check_dead(d: &Discovery, scan: &UsageScan, now: DateTime<Utc>, out: &mut Vec<Finding>) {
    for s in &d.skills {
        if s.disabled {
            continue;
        }
        let used = lookup_usage(scan, s).map(|u| u.count).unwrap_or(0) > 0;
        if used {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&s.path) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let modified: DateTime<Utc> = modified.into();
        let age = (now - modified).num_days();
        if age > DEAD_SKILL_AGE_DAYS {
            let dir = s
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            out.push(Finding {
                severity: Severity::Warn,
                code: "W004",
                title: format!("dead skill — {} (never used, last touched {age}d ago)", s.name),
                why: "it occupies a skill slot and description tokens in every session without ever firing".into(),
                fix: Some(format!("# unused since install — consider: rm -r \"{dir}\"")),
                skill: Some(s.name.clone()),
            });
        }
    }
}

fn check_heavy_skills(d: &Discovery, out: &mut Vec<Finding>) {
    for s in &d.skills {
        if s.est_tokens > SKILL_TOKEN_BUDGET {
            out.push(Finding {
                severity: Severity::Warn,
                code: "W006",
                title: format!("heavy skill — {} (~{} tokens)", s.name, s.est_tokens),
                why: "the whole SKILL.md body is loaded on trigger; heavy bodies eat the context window".into(),
                fix: Some(format!(
                    "# {} SKILL.md ~{} tokens — move reference material into a references/ dir",
                    s.name, s.est_tokens
                )),
                skill: Some(s.name.clone()),
            });
        }
    }
}

fn check_history_gap(
    d: &Discovery,
    scan: &UsageScan,
    history: &HashMap<String, crate::history::HistoryStats>,
    out: &mut Vec<Finding>,
) {
    for s in &d.skills {
        if s.disabled {
            continue;
        }
        if lookup_usage(scan, s).map(|u| u.count).unwrap_or(0) > 0 {
            continue;
        }
        let Some(h) = history.get(&s.name) else {
            continue;
        };
        if h.count == 0 {
            continue;
        }
        out.push(Finding {
            severity: Severity::Warn,
            code: "W010",
            title: format!(
                "usage data incomplete — {} typed {}× but absent from transcripts",
                s.name, h.count
            ),
            why: "your typed history proves this skill fires; transcripts were likely rotated or --projects-dir points at the wrong root, so its heat is understated".into(),
            fix: None,
            skill: Some(s.name.clone()),
        });
    }
}

fn check_claude_md(md: &ClaudeMdInfo, d: &Discovery, out: &mut Vec<Finding>) {
    for imp in &md.missing_imports {
        out.push(Finding {
            severity: Severity::Error,
            code: "E002",
            title: format!("missing @import — {imp} (in {})", md.path.display()),
            why:
                "the import silently resolves to nothing; instructions you think are loaded are not"
                    .into(),
            fix: Some(format!(
                "$EDITOR \"{}\"  # fix or remove @{imp}",
                md.path.display()
            )),
            skill: None,
        });
    }
    let est_tokens = md.resolved_chars / 4;
    if est_tokens > CLAUDE_MD_TOKEN_BUDGET {
        out.push(Finding {
            severity: Severity::Warn,
            code: "W005",
            title: format!("CLAUDE.md over budget — ~{est_tokens} tokens incl. @imports ({})", md.path.display()),
            why: "this is paid on every single turn; @imports count even though most tools forget them".into(),
            fix: Some(format!("# CLAUDE.md ~{est_tokens} tokens — trim or split rarely-needed sections")),
            skill: None,
        });
    }
    let installed: BTreeSet<&str> = d
        .skills
        .iter()
        .flat_map(|s| [s.name.as_str(), s.dir_name.as_str()])
        .collect();
    for cmd in slash_refs(&md.content) {
        if installed.contains(cmd.as_str()) || BUILTIN_COMMANDS.contains(&cmd.as_str()) {
            continue;
        }
        out.push(Finding {
            severity: Severity::Warn,
            code: "W007",
            title: format!("drift — /{cmd} referenced in {} but not installed", md.path.display()),
            why: "the instruction points at a skill that does not exist; the model will flounder or guess".into(),
            fix: Some(format!("$EDITOR \"{}\"  # /{cmd} referenced but not installed", md.path.display())),
            skill: None,
        });
    }
}

fn slash_refs(content: &str) -> BTreeSet<String> {
    let chars: Vec<char> = content.chars().collect();
    let mut out = BTreeSet::new();
    for (i, &c) in chars.iter().enumerate() {
        if c != '/' {
            continue;
        }
        let prev_ok = i == 0
            || matches!(
                chars[i - 1],
                ' ' | '`' | '(' | '"' | '\'' | '\n' | '\t' | '*' | '[' | '|'
            );
        if !prev_ok {
            continue;
        }
        let mut j = i + 1;
        while j < chars.len()
            && (chars[j].is_ascii_lowercase()
                || chars[j].is_ascii_digit()
                || chars[j] == '-'
                || chars[j] == ':')
        {
            j += 1;
        }
        // skip path segments like /tmp/foo
        if j < chars.len() && chars[j] == '/' {
            continue;
        }
        if j > i + 2 {
            out.insert(chars[i + 1..j].iter().collect());
        }
    }
    out
}

pub fn lookup_usage<'a>(
    scan: &'a UsageScan,
    skill: &crate::model::Skill,
) -> Option<&'a UsageStats> {
    scan.per_skill
        .get(&skill.name)
        .or_else(|| scan.per_skill.get(&skill.dir_name))
}

pub fn exit_code(findings: &[Finding]) -> i32 {
    if findings.iter().any(|f| f.severity == Severity::Error) {
        2
    } else if findings.iter().any(|f| f.severity == Severity::Warn) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_md::ClaudeMdInfo;
    use crate::discover::Discovery;
    use crate::model::{Skill, SkillSource, UsageStats};
    use crate::usage::UsageScan;
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap()
    }

    fn skill(name: &str) -> Skill {
        Skill {
            name: name.into(),
            dir_name: name.into(),
            source: SkillSource::User,
            path: PathBuf::from(format!("/s/{name}/SKILL.md")),
            description: Some("d".into()),
            frontmatter_ok: true,
            body: "body".into(),
            est_tokens: 100,
            always_on_tokens: 10,
            disabled: false,
        }
    }

    fn empty_scan() -> UsageScan {
        UsageScan {
            per_skill: HashMap::new(),
            files_total: 0,
            files_rescanned: 0,
        }
    }

    #[test]
    fn broken_frontmatter_is_e001() {
        let mut s = skill("broken");
        s.frontmatter_ok = false;
        let d = Discovery {
            skills: vec![s],
            debris: vec![],
        };
        let findings = run(&d, &empty_scan(), &[], &HashMap::new(), now());
        assert!(
            findings
                .iter()
                .any(|f| f.code == "E001" && f.severity == Severity::Error)
        );
    }

    #[test]
    fn missing_description_is_w001() {
        let mut s = skill("nodesc");
        s.description = None;
        let d = Discovery {
            skills: vec![s],
            debris: vec![],
        };
        let findings = run(&d, &empty_scan(), &[], &HashMap::new(), now());
        assert!(findings.iter().any(|f| f.code == "W001"));
    }

    #[test]
    fn shadowed_skill_is_w002() {
        let mut a = skill("dup");
        let mut b = skill("dup");
        a.source = SkillSource::User;
        b.source = SkillSource::Project;
        let d = Discovery {
            skills: vec![a, b],
            debris: vec![],
        };
        let findings = run(&d, &empty_scan(), &[], &HashMap::new(), now());
        assert!(findings.iter().any(|f| f.code == "W002"));
    }

    #[test]
    fn debris_is_w003_with_fix() {
        let d = Discovery {
            skills: vec![],
            debris: vec![PathBuf::from("/s/leftover")],
        };
        let findings = run(&d, &empty_scan(), &[], &HashMap::new(), now());
        let f = findings.iter().find(|f| f.code == "W003").unwrap();
        assert!(f.fix.as_deref().unwrap().contains("/s/leftover"));
    }

    #[test]
    fn used_skill_is_not_dead() {
        let d = Discovery {
            skills: vec![skill("alive")],
            debris: vec![],
        };
        let mut scan = empty_scan();
        scan.per_skill.insert(
            "alive".into(),
            UsageStats {
                count: 3,
                last_used: Some(now()),
                ..Default::default()
            },
        );
        let findings = run(&d, &scan, &[], &HashMap::new(), now());
        assert!(!findings.iter().any(|f| f.code == "W004"));
    }

    #[test]
    fn missing_import_is_e002_and_budget_is_w005() {
        let d = Discovery::default();
        let mds = vec![ClaudeMdInfo {
            path: PathBuf::from("/r/CLAUDE.md"),
            content: "x".into(),
            resolved_chars: 30_000, // 7500 est tokens > 5000 budget
            missing_imports: vec!["docs/gone.md".into()],
        }];
        let findings = run(&d, &empty_scan(), &mds, &HashMap::new(), now());
        assert!(findings.iter().any(|f| f.code == "E002"));
        assert!(findings.iter().any(|f| f.code == "W005"));
    }

    #[test]
    fn heavy_skill_is_w006() {
        let mut s = skill("heavy");
        s.est_tokens = 5000;
        let d = Discovery {
            skills: vec![s],
            debris: vec![],
        };
        let findings = run(&d, &empty_scan(), &[], &HashMap::new(), now());
        assert!(findings.iter().any(|f| f.code == "W006"));
    }

    #[test]
    fn claude_md_drift_is_w007_builtins_excluded() {
        let d = Discovery {
            skills: vec![skill("cfo")],
            debris: vec![],
        };
        let mds = vec![ClaudeMdInfo {
            path: PathBuf::from("/r/CLAUDE.md"),
            content: "Use /cfo and /ghost-skill and /clear and /tmp/foo".into(),
            resolved_chars: 10,
            missing_imports: vec![],
        }];
        let findings = run(&d, &empty_scan(), &mds, &HashMap::new(), now());
        let drift: Vec<_> = findings.iter().filter(|f| f.code == "W007").collect();
        assert_eq!(drift.len(), 1);
        assert!(drift[0].title.contains("ghost-skill"));
    }

    #[test]
    fn disabled_skill_is_never_dead_w004() {
        // skill() paths are fake (/s/..) so metadata() fails and W004 can't
        // fire that way — use a REAL file and a far-future `now` to age it
        // past DEAD_SKILL_AGE_DAYS for both skills; only the enabled one dies.
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("SKILL.md");
        std::fs::write(&p, "x").unwrap();
        let mut off = skill("offplugin");
        off.disabled = true;
        off.path = p.clone();
        let mut alive = skill("deadalive");
        alive.path = p;
        let d = Discovery {
            skills: vec![off, alive],
            debris: vec![],
        };
        let future = Utc.with_ymd_and_hms(2027, 6, 10, 12, 0, 0).unwrap();
        let f = run(&d, &empty_scan(), &[], &HashMap::new(), future);
        let dead: Vec<_> = f.iter().filter(|x| x.code == "W004").collect();
        assert_eq!(dead.len(), 1, "only the enabled skill goes dead");
        assert_eq!(dead[0].skill.as_deref(), Some("deadalive"));
    }

    #[test]
    fn history_without_transcripts_is_w010() {
        let d = Discovery {
            skills: vec![skill("rotated"), skill("seen")],
            debris: vec![],
        };
        let mut scan = empty_scan();
        scan.per_skill.insert(
            "seen".into(),
            UsageStats {
                count: 1,
                last_used: Some(now()),
                ..Default::default()
            },
        );
        let mut hist: HashMap<String, crate::history::HistoryStats> = HashMap::new();
        hist.insert(
            "rotated".into(),
            crate::history::HistoryStats {
                count: 26,
                last_used: None,
            },
        );
        hist.insert(
            "seen".into(),
            crate::history::HistoryStats {
                count: 5,
                last_used: None,
            },
        );
        let f = run(&d, &scan, &[], &hist, now());
        let w010: Vec<_> = f.iter().filter(|x| x.code == "W010").collect();
        assert_eq!(w010.len(), 1);
        assert!(w010[0].title.contains("rotated"));
        assert!(w010[0].title.contains("26×"));
    }

    #[test]
    fn disabled_skill_does_not_fire_w010() {
        // A disabled skill can never be invoked, so "history says it fired but
        // transcripts don't" is meaningless — W010 must not fire for it.
        let mut s = skill("offhistory");
        s.disabled = true;
        let d = Discovery {
            skills: vec![s],
            debris: vec![],
        };
        let mut hist: HashMap<String, crate::history::HistoryStats> = HashMap::new();
        hist.insert(
            "offhistory".into(),
            crate::history::HistoryStats {
                count: 5,
                last_used: None,
            },
        );
        let f = run(&d, &empty_scan(), &[], &hist, now());
        assert!(
            !f.iter().any(|x| x.code == "W010"),
            "disabled skill must not produce W010"
        );
    }

    #[test]
    fn exit_codes_follow_severity() {
        use crate::model::Severity;
        assert_eq!(exit_code(&[]), 0);
        assert_eq!(
            exit_code(&[Finding {
                severity: Severity::Warn,
                code: "W001",
                title: "t".into(),
                why: "w".into(),
                fix: None,
                skill: None,
            }]),
            1
        );
        assert_eq!(
            exit_code(&[Finding {
                severity: Severity::Error,
                code: "E001",
                title: "t".into(),
                why: "w".into(),
                fix: None,
                skill: None,
            }]),
            2
        );
    }
}
