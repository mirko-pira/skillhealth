use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use skillhealth_core::model::{SkillSource, Temperature};
use skillhealth_core::report::Report;

pub fn render_overview(report: &Report, color: bool) -> String {
    let s = &report.summary;
    let mut out = format!(
        "skillhealth · {} skills · {} hot · {} warm · {} cold · {} dead\n",
        s.total, s.hot, s.warm, s.cold, s.dead
    );
    let v = &report.view;
    let project = v
        .project_label
        .as_deref()
        .map(|l| format!(" ({l})"))
        .unwrap_or_default();
    out.push_str(&format!(
        "scope: {}{} · lens: {}\n",
        v.scope.label(),
        project,
        v.lens.label()
    ));
    if s.transcript_files == 0 {
        out.push_str(&format!(
            "! no transcripts at {} — heat unavailable (--projects-dir?)\n",
            v.projects_dir.display()
        ));
    }
    if s.dead_always_on > 0 {
        let msg = format!(
            "⚠ ~{} tok/session wasted on skills you never use",
            format_tokens(s.dead_always_on)
        );
        if color {
            out.push_str(&msg.bright_yellow().to_string());
        } else {
            out.push_str(&msg);
        }
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&format!(
        "  {:<2} {:<34}{:<10}{:>5}  {:<11}{:>9}\n",
        "", "SKILL", "SOURCE", "USES", "LAST USED", "ALWAYS-ON"
    ));
    let mut skills: Vec<_> = report.skills.iter().collect();
    skills.sort_by(|a, b| b.usage.count.cmp(&a.usage.count).then(a.name.cmp(&b.name)));
    for sk in skills {
        let source_cell = if sk.disabled {
            format!("{} off", source_label(&sk.source))
        } else {
            source_label(&sk.source)
        };
        out.push_str(&format!(
            "  {} {:<34}{:<10}{:>5}  {:<11}{:>9}\n",
            temp_symbol(sk.temperature, color),
            truncate(&sk.name, 32),
            source_cell,
            sk.usage.count,
            ago(sk.usage.last_used, report.generated_at),
            format_tokens(sk.cost.always_on),
        ));
    }
    if s.errors + s.warnings > 0 {
        out.push_str(&format!(
            "\n! {} warning(s), {} error(s) — run `skillhealth doctor`\n",
            s.warnings, s.errors
        ));
    } else {
        out.push_str("\n✓ no issues found\n");
    }
    out.push_str(&format!(
        "always-on total: {} tok/session across {} skills\n",
        format_tokens(s.always_on_total),
        s.total - s.disabled
    ));
    if s.dead_always_on > 0 {
        let loaded_dead = report
            .skills
            .iter()
            .filter(|sk| sk.temperature == Temperature::Dead && !sk.disabled)
            .count();
        let pct = (s.dead_always_on * 100 + s.always_on_total / 2)
            .checked_div(s.always_on_total)
            .unwrap_or(0);
        let plural = if loaded_dead == 1 { "" } else { "s" };
        out.push_str(&format!(
            "  └ {} ({}%) burned by {} dead skill{}\n",
            format_tokens(s.dead_always_on),
            pct,
            loaded_dead,
            plural
        ));
    }
    out
}

fn temp_symbol(t: Temperature, color: bool) -> String {
    let sym = match t {
        Temperature::Dead => "○",
        _ => "●",
    };
    if !color {
        return sym.to_string();
    }
    match t {
        Temperature::Hot => sym.red().to_string(),
        Temperature::Warm => sym.yellow().to_string(),
        Temperature::Cold => sym.blue().to_string(),
        Temperature::Dead => sym.dimmed().to_string(),
    }
}

pub(crate) fn source_label(s: &SkillSource) -> String {
    match s {
        SkillSource::User => "user".into(),
        SkillSource::Project => "project".into(),
        SkillSource::Plugin(_) => "plugin".into(),
    }
}

pub fn ago(t: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    match t {
        None => "never".into(),
        Some(t) => {
            let d = now.signed_duration_since(t);
            if d.num_days() >= 1 {
                format!("{}d ago", d.num_days())
            } else if d.num_hours() >= 1 {
                format!("{}h ago", d.num_hours())
            } else {
                "just now".into()
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}…")
    }
}

pub fn format_tokens(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use skillhealth_core::model::{Temperature, UsageStats};
    use skillhealth_core::report::{Cost, Report, ReportSkill, Summary};
    use std::path::PathBuf;

    fn fixture_report() -> Report {
        let now = Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap();
        Report {
            schema_version: 1,
            generated_at: now,
            summary: Summary {
                total: 2,
                hot: 1,
                warm: 0,
                cold: 0,
                dead: 1,
                errors: 0,
                warnings: 1,
                transcript_files: 1,
                files_rescanned: 0,
                always_on_total: 128,
                dead_always_on: 8,
                disabled: 0,
            },
            skills: vec![
                ReportSkill {
                    name: "cfo".into(),
                    source: skillhealth_core::model::SkillSource::User,
                    path: PathBuf::from("/s/cfo/SKILL.md"),
                    description: Some("CFO".into()),
                    temperature: Temperature::Hot,
                    usage: UsageStats {
                        count: 9,
                        last_used: Some(Utc.with_ymd_and_hms(2026, 6, 9, 0, 0, 0).unwrap()),
                        ..Default::default()
                    },
                    est_tokens: 1234,
                    cost: Cost {
                        always_on: 120,
                        on_fire: 1234,
                    },
                    disabled: false,
                    history: None,
                    weekly: [0, 0, 0, 0, 0, 1, 0, 2, 1, 0, 3, 2],
                },
                ReportSkill {
                    name: "old".into(),
                    source: skillhealth_core::model::SkillSource::User,
                    path: PathBuf::from("/s/old/SKILL.md"),
                    description: None,
                    temperature: Temperature::Dead,
                    usage: UsageStats::default(),
                    est_tokens: 80,
                    cost: Cost {
                        always_on: 8,
                        on_fire: 80,
                    },
                    disabled: false,
                    history: None,
                    weekly: [0; 12],
                },
            ],
            edges: vec![],
            findings: vec![],
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

    #[test]
    fn overview_snapshot_plain() {
        insta::assert_snapshot!(render_overview(&fixture_report(), false));
    }

    #[test]
    fn no_transcripts_warning_when_files_total_zero() {
        let mut r = fixture_report();
        r.summary.transcript_files = 0;
        let out = render_overview(&r, false);
        assert!(out.contains("no transcripts at"));
        assert!(out.contains("--projects-dir"));
    }

    #[test]
    fn disabled_skill_and_project_label_render_in_overview() {
        let mut r = fixture_report();
        r.skills[1].disabled = true; // "old" skill — User source → cell becomes "user off"
        r.summary.disabled = 1;
        r.view.project_label = Some("my-repo".into());
        r.view.scope = skillhealth_core::view::Scope::Project;
        let out = render_overview(&r, false);
        assert!(out.contains("scope: project (my-repo)"), "got:\n{out}");
        assert!(
            out.contains(" off"),
            "disabled SOURCE cell missing — got:\n{out}"
        );
    }

    #[test]
    fn waste_headline_callout_and_footer_in_overview() {
        // fixture: cfo hot (always_on 120) + old dead enabled (always_on 8)
        //          → 8 of 128 always-on tokens wasted on 1 dead skill.
        let out = render_overview(&fixture_report(), false);
        assert!(
            out.contains("wasted on skills you never use"),
            "missing top waste callout — got:\n{out}"
        );
        assert!(
            out.contains("burned by 1 dead skill"),
            "missing footer waste breakdown (count + singular) — got:\n{out}"
        );
    }

    #[test]
    fn no_waste_callout_when_nothing_dead_is_loaded() {
        let mut r = fixture_report();
        // mark the dead skill disabled: it loads nothing, so there is no waste
        r.skills[1].disabled = true;
        r.summary.dead_always_on = 0;
        let out = render_overview(&r, false);
        assert!(
            !out.contains("wasted on skills you never use"),
            "waste callout must be hidden when no dead tokens are loaded — got:\n{out}"
        );
    }

    #[test]
    fn detail_history_line_absent_from_transcripts() {
        use skillhealth_core::history::HistoryStats;
        use skillhealth_core::model::UsageStats;
        use skillhealth_core::report::{Cost, ReportSkill};
        let r = fixture_report();
        let sk = ReportSkill {
            name: "old".into(),
            source: skillhealth_core::model::SkillSource::User,
            path: PathBuf::from("/s/old/SKILL.md"),
            description: None,
            temperature: Temperature::Dead,
            usage: UsageStats::default(),
            est_tokens: 80,
            cost: Cost {
                always_on: 8,
                on_fire: 80,
            },
            disabled: false,
            history: Some(HistoryStats {
                count: 3,
                last_used: Some(Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap()),
            }),
            weekly: [0; 12],
        };
        let out = super::super::detail::render_detail(&r, &sk);
        assert!(out.contains("typed"), "got:\n{out}");
        assert!(out.contains("absent from transcripts"), "got:\n{out}");
    }

    #[test]
    fn detail_disabled_plugin_line_rendered() {
        use skillhealth_core::model::UsageStats;
        use skillhealth_core::report::{Cost, ReportSkill};
        let r = fixture_report();
        let sk = ReportSkill {
            name: "cfo".into(),
            source: skillhealth_core::model::SkillSource::User,
            path: PathBuf::from("/s/cfo/SKILL.md"),
            description: Some("CFO".into()),
            temperature: Temperature::Hot,
            usage: UsageStats {
                count: 9,
                last_used: Some(Utc.with_ymd_and_hms(2026, 6, 9, 0, 0, 0).unwrap()),
                ..Default::default()
            },
            est_tokens: 1234,
            cost: Cost {
                always_on: 120,
                on_fire: 1234,
            },
            disabled: true,
            history: None,
            weekly: [0; 12],
        };
        let out = super::super::detail::render_detail(&r, &sk);
        assert!(out.contains("disabled"), "got:\n{out}");
        assert!(out.contains("off"), "got:\n{out}");
    }
}
