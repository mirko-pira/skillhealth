use super::terminal::{ago, format_tokens, source_label};
use skillhealth_core::report::Report;

pub fn render_mermaid(report: &Report) -> String {
    let mut out = String::from("graph TD\n");
    for sk in &report.skills {
        out.push_str(&format!("  {}[\"{}\"]\n", node_id(&sk.name), sk.name));
    }
    for e in &report.edges {
        out.push_str(&format!("  {} --> {}\n", node_id(&e.from), node_id(&e.to)));
    }
    out
}

pub fn render_markdown(report: &Report) -> String {
    let s = &report.summary;
    let v = &report.view;
    let mut out = format!(
        "# skillhealth report\n\n{} skills — {} hot · {} warm · {} cold · {} dead\n\n",
        s.total, s.hot, s.warm, s.cold, s.dead
    );
    let project = v
        .project_label
        .as_deref()
        .map(|l| format!(" ({l})"))
        .unwrap_or_default();
    out.push_str(&format!(
        "scope: {}{} · lens: {}\n\n",
        v.scope.label(),
        project,
        v.lens.label()
    ));
    if s.transcript_files == 0 {
        out.push_str(&format!(
            "> ⚠ no transcripts at `{}` — heat unavailable (`--projects-dir`?)\n\n",
            v.projects_dir.display()
        ));
    }
    out.push_str("| SKILL | SOURCE | TEMP | USES | LAST USED | ALWAYS-ON |\n");
    out.push_str("|---|---|---|---:|---|---:|\n");
    let mut skills: Vec<_> = report.skills.iter().collect();
    skills.sort_by(|a, b| b.usage.count.cmp(&a.usage.count).then(a.name.cmp(&b.name)));
    for sk in skills {
        let source_cell = if sk.disabled {
            format!("{} off", source_label(&sk.source))
        } else {
            source_label(&sk.source)
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            sk.name,
            source_cell,
            format!("{:?}", sk.temperature).to_lowercase(),
            sk.usage.count,
            ago(sk.usage.last_used, report.generated_at),
            format_tokens(sk.cost.always_on),
        ));
    }
    out.push_str(&format!(
        "\nalways-on total: {} tok/session across {} skills\n",
        format_tokens(s.always_on_total),
        s.total - s.disabled
    ));
    if !report.findings.is_empty() {
        out.push_str("\n## Findings\n\n");
        for f in &report.findings {
            out.push_str(&format!("- **[{}]** {} — {}\n", f.code, f.title, f.why));
        }
    }
    out.push_str("\n## Graph\n\n```mermaid\n");
    out.push_str(&render_mermaid(report));
    out.push_str("```\n");
    out
}

pub fn node_id(name: &str) -> String {
    name.replace([':', '-', '.', ' ', '/'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use skillhealth_core::model::{SkillSource, Temperature, UsageStats};
    use skillhealth_core::report::{Cost, ReportSkill, Summary};
    use skillhealth_core::view::{Lens, Scope, ViewInfo};
    use std::path::PathBuf;

    fn fixture() -> Report {
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
                warnings: 0,
                transcript_files: 1,
                files_rescanned: 0,
                always_on_total: 1300,
                dead_always_on: 0,
                disabled: 1,
            },
            skills: vec![
                ReportSkill {
                    name: "cfo".into(),
                    source: SkillSource::User,
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
                    weekly: [0; 12],
                },
                ReportSkill {
                    name: "writing-plans".into(),
                    source: SkillSource::Plugin("superpowers".into()),
                    path: PathBuf::from("/p/writing-plans/SKILL.md"),
                    description: None,
                    temperature: Temperature::Dead,
                    usage: UsageStats::default(),
                    est_tokens: 80,
                    cost: Cost {
                        always_on: 8,
                        on_fire: 80,
                    },
                    disabled: true,
                    history: None,
                    weekly: [0; 12],
                },
            ],
            edges: vec![],
            findings: vec![],
            unmatched_usage: vec![],
            view: ViewInfo {
                scope: Scope::All,
                lens: Lens::Global,
                project_root: None,
                project_label: None,
                projects_dir: PathBuf::from("/projects"),
            },
        }
    }

    #[test]
    fn markdown_has_v02_parity_elements() {
        let out = render_markdown(&fixture());
        // scope/lens header line (was missing entirely in v0.1)
        assert!(out.contains("scope: all · lens: global"), "got:\n{out}");
        // ALWAYS-ON column from cost.always_on, not the old raw TOKENS/est_tokens
        assert!(out.contains("| ALWAYS-ON |"), "got:\n{out}");
        assert!(
            !out.contains("TOKENS"),
            "old TOKENS column still present:\n{out}"
        );
        // always-on total footer
        assert!(out.contains("always-on total:"), "got:\n{out}");
        // disabled skill: human source label + off badge (not Debug Plugin("..."))
        assert!(out.contains("plugin off"), "got:\n{out}");
        assert!(!out.contains("Plugin("), "Debug source leaked:\n{out}");
        // temperature is a lowercase label, not Debug-cased "Hot"/"Dead"
        assert!(out.contains("| hot |"), "got:\n{out}");
        assert!(
            !out.contains("Hot") && !out.contains("Dead"),
            "Debug-cased temperature leaked:\n{out}"
        );
    }

    #[test]
    fn markdown_project_scope_shows_label() {
        let mut r = fixture();
        r.view.scope = Scope::Project;
        r.view.lens = Lens::Project;
        r.view.project_label = Some("demo-app".into());
        let out = render_markdown(&r);
        assert!(
            out.contains("scope: project (demo-app) · lens: project"),
            "got:\n{out}"
        );
    }
}
