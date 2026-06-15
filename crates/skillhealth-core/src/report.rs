use crate::discover::Discovery;
use crate::doctor::{Finding, lookup_usage};
use crate::graph::Edge;
use crate::model::{Severity, SkillSource, Temperature, UsageStats, iso_week_key, temperature};
use crate::usage::UsageScan;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Cost {
    /// Description tokens — paid in the system prompt every session.
    pub always_on: usize,
    /// Body tokens — paid when the skill fires.
    pub on_fire: usize,
}

#[derive(Debug, Serialize)]
pub struct ReportSkill {
    pub name: String,
    pub source: SkillSource,
    pub path: PathBuf,
    pub description: Option<String>,
    pub temperature: Temperature,
    pub usage: UsageStats,
    pub est_tokens: usize,
    pub cost: Cost,
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<crate::history::HistoryStats>,
    /// Mentions per ISO week over the last 12 weeks, oldest→newest, relative
    /// to `generated_at`. Additive in --json; fuels the TUI sparklines.
    pub weekly: [u32; 12],
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub total: usize,
    pub hot: usize,
    pub warm: usize,
    pub cold: usize,
    pub dead: usize,
    pub errors: usize,
    pub warnings: usize,
    pub transcript_files: usize,
    pub files_rescanned: usize,
    pub always_on_total: usize,
    /// Always-on tokens burned every session by enabled dead skills — the
    /// context you pay for and never invoke. Subset of `always_on_total`.
    pub dead_always_on: usize,
    pub disabled: usize,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub summary: Summary,
    pub skills: Vec<ReportSkill>,
    pub edges: Vec<Edge>,
    pub findings: Vec<Finding>,
    /// Usage keys in transcripts that match no installed skill (likely uninstalled skills).
    pub unmatched_usage: Vec<String>,
    pub view: crate::view::ViewInfo,
}

/// Map absolute ISO-week counts onto a 12-slot window ending at `now`'s week.
/// Slot 0 = 11 weeks ago, slot 11 = the current ISO week.
pub fn weekly_buckets(week_counts: &BTreeMap<String, u32>, now: DateTime<Utc>) -> [u32; 12] {
    let mut out = [0u32; 12];
    for (i, slot) in out.iter_mut().enumerate() {
        let in_week = now - chrono::Duration::weeks(11 - i as i64);
        if let Some(n) = week_counts.get(&iso_week_key(in_week)) {
            *slot = *n;
        }
    }
    out
}

fn in_scope(scope: crate::view::Scope, source: &SkillSource) -> bool {
    match scope {
        crate::view::Scope::All => true,
        crate::view::Scope::Project => matches!(source, SkillSource::Project),
        crate::view::Scope::User => !matches!(source, SkillSource::Project),
    }
}

pub fn build(
    d: &Discovery,
    scan: &UsageScan,
    edges: Vec<Edge>,
    findings: Vec<Finding>,
    history: &std::collections::HashMap<String, crate::history::HistoryStats>,
    view: crate::view::ViewInfo,
    now: DateTime<Utc>,
) -> Report {
    let visible: Vec<&crate::model::Skill> = d
        .skills
        .iter()
        .filter(|s| in_scope(view.scope, &s.source))
        .collect();

    let mut summary = Summary {
        total: visible.len(),
        transcript_files: scan.files_total,
        files_rescanned: scan.files_rescanned,
        ..Default::default()
    };

    // Build matched set from ALL skills (not just visible) — an uninstalled-skill
    // key is not scope-dependent. Plan deviation note (b).
    let mut matched: BTreeSet<&str> = BTreeSet::new();
    for s in &d.skills {
        if scan.per_skill.contains_key(&s.name) {
            matched.insert(s.name.as_str());
        }
        if scan.per_skill.contains_key(&s.dir_name) {
            matched.insert(s.dir_name.as_str());
        }
    }

    let skills: Vec<ReportSkill> = visible
        .iter()
        .map(|s| {
            let usage = lookup_usage(scan, s).cloned().unwrap_or_default();
            let weekly = weekly_buckets(&usage.week_counts, now);
            let temp = temperature(usage.last_used, now);
            match temp {
                Temperature::Hot => summary.hot += 1,
                Temperature::Warm => summary.warm += 1,
                Temperature::Cold => summary.cold += 1,
                Temperature::Dead => summary.dead += 1,
            }
            ReportSkill {
                name: s.name.clone(),
                source: s.source.clone(),
                path: s.path.clone(),
                description: s.description.clone(),
                temperature: temp,
                usage,
                est_tokens: s.est_tokens,
                cost: Cost {
                    always_on: s.always_on_tokens,
                    on_fire: s.est_tokens,
                },
                disabled: s.disabled,
                history: history.get(&s.name).copied(),
                weekly,
            }
        })
        .collect();

    summary.always_on_total = visible
        .iter()
        .filter(|s| !s.disabled)
        .map(|s| s.always_on_tokens)
        .sum();
    summary.dead_always_on = skills
        .iter()
        .filter(|s| s.temperature == Temperature::Dead && !s.disabled)
        .map(|s| s.cost.always_on)
        .sum();
    summary.disabled = visible.iter().filter(|s| s.disabled).count();

    // Build visible key sets for scope-filtering findings and edges.
    let visible_keys: BTreeSet<&str> = visible
        .iter()
        .flat_map(|s| [s.name.as_str(), s.dir_name.as_str()])
        .collect();
    let findings: Vec<Finding> = findings
        .into_iter()
        .filter(|f| f.skill.as_deref().is_none_or(|s| visible_keys.contains(s)))
        .collect();

    summary.errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    summary.warnings = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .count();

    let visible_names: BTreeSet<&str> = visible.iter().map(|s| s.name.as_str()).collect();
    let edges: Vec<Edge> = edges
        .into_iter()
        .filter(|e| {
            visible_names.contains(e.from.as_str()) && visible_names.contains(e.to.as_str())
        })
        .collect();

    let unmatched_usage: Vec<String> = scan
        .per_skill
        .keys()
        .filter(|k| !matched.contains(k.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Report {
        schema_version: SCHEMA_VERSION,
        generated_at: now,
        summary,
        skills,
        edges,
        findings,
        unmatched_usage,
        view,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            body: String::new(),
            est_tokens: 50,
            always_on_tokens: 10,
            disabled: false,
        }
    }

    fn view(scope: crate::view::Scope) -> crate::view::ViewInfo {
        crate::view::ViewInfo {
            scope,
            lens: crate::view::Lens::Global,
            project_root: None,
            project_label: None,
            projects_dir: PathBuf::from("/projects"),
        }
    }

    #[test]
    fn weekly_buckets_empty_is_all_zeros() {
        let wc = std::collections::BTreeMap::new();
        assert_eq!(weekly_buckets(&wc, now()), [0u32; 12]);
    }

    #[test]
    fn weekly_buckets_places_current_week_last_and_oldest_first() {
        let mut wc = std::collections::BTreeMap::new();
        wc.insert("2026-W24".into(), 3); // now (2026-06-10) is ISO week 24
        wc.insert("2026-W13".into(), 1); // 11 weeks earlier → bucket 0
        let b = weekly_buckets(&wc, now());
        assert_eq!(b[11], 3);
        assert_eq!(b[0], 1);
        assert_eq!(b.iter().sum::<u32>(), 4);
    }

    #[test]
    fn weekly_buckets_drops_weeks_older_than_window() {
        let mut wc = std::collections::BTreeMap::new();
        wc.insert("2026-W12".into(), 9); // 12 weeks before W24 → outside
        assert_eq!(weekly_buckets(&wc, now()), [0u32; 12]);
    }

    #[test]
    fn report_skills_carry_weekly() {
        let d = Discovery {
            skills: vec![skill("hot-one")],
            debris: vec![],
        };
        let mut scan = UsageScan {
            per_skill: HashMap::new(),
            files_total: 0,
            files_rescanned: 0,
        };
        let mut stats = UsageStats {
            count: 2,
            last_used: Some(Utc.with_ymd_and_hms(2026, 6, 9, 0, 0, 0).unwrap()),
            ..Default::default()
        };
        stats.week_counts.insert("2026-W24".into(), 2);
        scan.per_skill.insert("hot-one".into(), stats);
        let r = build(
            &d,
            &scan,
            vec![],
            vec![],
            &HashMap::new(),
            view(crate::view::Scope::All),
            now(),
        );
        assert_eq!(r.skills[0].weekly[11], 2);
        assert_eq!(r.skills[0].weekly.iter().sum::<u32>(), 2);
    }

    #[test]
    fn builds_report_with_temperatures_and_summary() {
        let d = Discovery {
            skills: vec![skill("hot-one"), skill("dead-one")],
            debris: vec![],
        };
        let mut scan = UsageScan {
            per_skill: HashMap::new(),
            files_total: 3,
            files_rescanned: 1,
        };
        scan.per_skill.insert(
            "hot-one".into(),
            UsageStats {
                count: 9,
                last_used: Some(Utc.with_ymd_and_hms(2026, 6, 9, 0, 0, 0).unwrap()),
                ..Default::default()
            },
        );
        let r = build(
            &d,
            &scan,
            vec![],
            vec![],
            &HashMap::new(),
            view(crate::view::Scope::All),
            now(),
        );
        assert_eq!(r.schema_version, 1);
        assert_eq!(r.summary.total, 2);
        assert_eq!(r.summary.hot, 1);
        assert_eq!(r.summary.dead, 1);
        let hot = r.skills.iter().find(|s| s.name == "hot-one").unwrap();
        assert_eq!(hot.temperature, crate::model::Temperature::Hot);
        assert_eq!(hot.usage.count, 9);
    }

    #[test]
    fn unmatched_usage_keys_are_reported() {
        let d = Discovery {
            skills: vec![skill("real")],
            debris: vec![],
        };
        let mut scan = UsageScan {
            per_skill: HashMap::new(),
            files_total: 0,
            files_rescanned: 0,
        };
        scan.per_skill.insert(
            "ghost-removed-skill".into(),
            UsageStats {
                count: 4,
                last_used: None,
                ..Default::default()
            },
        );
        let r = build(
            &d,
            &scan,
            vec![],
            vec![],
            &HashMap::new(),
            view(crate::view::Scope::All),
            now(),
        );
        assert_eq!(r.unmatched_usage, vec!["ghost-removed-skill".to_string()]);
    }

    #[test]
    fn scope_filters_skills_findings_edges_and_summary() {
        use crate::model::SkillSource;
        let mut user = skill("mine");
        user.source = SkillSource::User;
        let mut proj = skill("theirs");
        proj.source = SkillSource::Project;
        let d = Discovery {
            skills: vec![user, proj],
            debris: vec![],
        };
        let findings = vec![
            crate::doctor::Finding {
                severity: crate::model::Severity::Warn,
                code: "W004",
                title: "dead skill — mine".into(),
                why: "w".into(),
                fix: None,
                skill: Some("mine".into()),
            },
            crate::doctor::Finding {
                severity: crate::model::Severity::Warn,
                code: "W003",
                title: "debris".into(),
                why: "w".into(),
                fix: None,
                skill: None,
            },
        ];
        let edges = vec![crate::graph::Edge {
            from: "mine".into(),
            to: "theirs".into(),
            kind: crate::graph::EdgeKind::SkillMention,
        }];
        let scan = UsageScan {
            per_skill: HashMap::new(),
            files_total: 0,
            files_rescanned: 0,
        };
        let r = build(
            &d,
            &scan,
            edges,
            findings,
            &HashMap::new(),
            view(crate::view::Scope::Project),
            now(),
        );
        assert_eq!(r.summary.total, 1);
        assert_eq!(r.skills[0].name, "theirs");
        // skill-specific finding about an out-of-scope skill is dropped;
        // skill-less findings stay
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].code, "W003");
        // edge with an out-of-scope endpoint is dropped
        assert!(r.edges.is_empty());
        assert_eq!(r.view.scope, crate::view::Scope::Project);
    }

    #[test]
    fn cost_disabled_history_and_always_on_total() {
        let mut a = skill("a");
        a.always_on_tokens = 30;
        let mut b = skill("b");
        b.always_on_tokens = 70;
        b.disabled = true;
        let d = Discovery {
            skills: vec![a, b],
            debris: vec![],
        };
        let scan = UsageScan {
            per_skill: HashMap::new(),
            files_total: 0,
            files_rescanned: 0,
        };
        let mut hist = HashMap::new();
        hist.insert(
            "a".into(),
            crate::history::HistoryStats {
                count: 4,
                last_used: None,
            },
        );
        let r = build(
            &d,
            &scan,
            vec![],
            vec![],
            &hist,
            view(crate::view::Scope::All),
            now(),
        );
        let ra = r.skills.iter().find(|s| s.name == "a").unwrap();
        assert_eq!(ra.cost.always_on, 30);
        assert_eq!(ra.cost.on_fire, 50); // skill() helper sets est_tokens 50
        assert_eq!(ra.history.unwrap().count, 4);
        let rb = r.skills.iter().find(|s| s.name == "b").unwrap();
        assert!(rb.disabled);
        assert!(rb.history.is_none());
        // disabled skills don't pay always-on (description never loaded)
        assert_eq!(r.summary.always_on_total, 30);
        assert_eq!(r.summary.disabled, 1);
    }

    #[test]
    fn dead_always_on_sums_only_enabled_dead_skills() {
        let mut hot = skill("hot");
        hot.always_on_tokens = 10;
        let mut dead_a = skill("dead-a");
        dead_a.always_on_tokens = 30;
        let mut dead_b = skill("dead-b");
        dead_b.always_on_tokens = 70;
        dead_b.disabled = true; // dead but disabled — never loaded, so no waste
        let d = Discovery {
            skills: vec![hot, dead_a, dead_b],
            debris: vec![],
        };
        let mut scan = UsageScan {
            per_skill: HashMap::new(),
            files_total: 1,
            files_rescanned: 0,
        };
        scan.per_skill.insert(
            "hot".into(),
            UsageStats {
                count: 3,
                last_used: Some(Utc.with_ymd_and_hms(2026, 6, 9, 0, 0, 0).unwrap()),
                ..Default::default()
            },
        );
        let r = build(
            &d,
            &scan,
            vec![],
            vec![],
            &HashMap::new(),
            view(crate::view::Scope::All),
            now(),
        );
        // only the enabled dead skill counts: hot pays but isn't waste,
        // disabled dead skill never loads so it pays nothing.
        assert_eq!(r.summary.dead_always_on, 30);
        assert_eq!(r.summary.always_on_total, 40);
    }
}
