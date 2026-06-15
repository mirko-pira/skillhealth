//! Test-only fixtures shared by app/view tests.
#![cfg(test)]

use chrono::{TimeZone, Utc};
use skillhealth_core::doctor::Finding;
use skillhealth_core::graph::{Edge, EdgeKind};
use skillhealth_core::model::{Severity, SkillSource, Temperature, UsageStats};
use skillhealth_core::report::{Cost, Report, ReportSkill, Summary};
use std::path::PathBuf;

pub fn skill(
    name: &str,
    source: SkillSource,
    temp: Temperature,
    count: u64,
    tokens: usize,
) -> ReportSkill {
    ReportSkill {
        name: name.into(),
        source,
        path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
        description: Some(format!("description of {name}")),
        temperature: temp,
        usage: UsageStats {
            count,
            ..Default::default()
        },
        est_tokens: tokens,
        cost: Cost {
            always_on: tokens / 10,
            on_fire: tokens,
        },
        disabled: false,
        history: None,
        weekly: if count > 0 {
            [0, 0, 0, 1, 0, 0, 2, 0, 1, 0, 3, 2]
        } else {
            [0; 12]
        },
    }
}

pub fn fixture_report() -> Report {
    let now = Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap();
    Report {
        schema_version: 1,
        generated_at: now,
        summary: Summary {
            total: 4,
            hot: 1,
            warm: 1,
            cold: 1,
            dead: 1,
            errors: 1,
            warnings: 1,
            transcript_files: 2,
            files_rescanned: 0,
            always_on_total: 606,
            dead_always_on: 60,
            disabled: 0,
        },
        skills: vec![
            skill("cfo", SkillSource::User, Temperature::Hot, 9, 1200),
            skill("finance", SkillSource::Project, Temperature::Warm, 4, 800),
            skill(
                "superpowers:writing-plans",
                SkillSource::Plugin("superpowers".into()),
                Temperature::Cold,
                2,
                4000,
            ),
            skill(
                "old-experiment",
                SkillSource::User,
                Temperature::Dead,
                0,
                60,
            ),
        ],
        edges: vec![Edge {
            from: "cfo".into(),
            to: "finance".into(),
            kind: EdgeKind::SkillMention,
        }],
        findings: vec![
            Finding {
                severity: Severity::Error,
                code: "E001",
                title: "broken frontmatter — old-experiment".into(),
                why: "SKILL.md has no parseable name/description".into(),
                fix: Some("$EDITOR \"/skills/old-experiment/SKILL.md\"".into()),
                skill: Some("old-experiment".into()),
            },
            Finding {
                severity: Severity::Warn,
                code: "W004",
                title: "dead skill — old-experiment".into(),
                why: "never fired and untouched for months".into(),
                fix: None,
                skill: Some("old-experiment".into()),
            },
        ],
        unmatched_usage: vec![],
        view: skillhealth_core::view::ViewInfo {
            scope: skillhealth_core::view::Scope::All,
            lens: skillhealth_core::view::Lens::Global,
            project_root: None,
            project_label: None,
            projects_dir: PathBuf::from("/projects"),
        },
    }
}
