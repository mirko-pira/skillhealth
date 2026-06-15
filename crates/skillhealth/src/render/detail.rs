use super::terminal::{ago, format_tokens};
use skillhealth_core::report::{Report, ReportSkill};

pub fn render_detail(report: &Report, skill: &ReportSkill) -> String {
    let mut out = format!("{}\n{}\n\n", skill.name, "─".repeat(skill.name.len()));
    if let Some(d) = &skill.description {
        out.push_str(&format!("{d}\n\n"));
    }
    out.push_str(&format!(
        "  state      {} ({} uses, last {})\n",
        format!("{:?}", skill.temperature).to_lowercase(),
        skill.usage.count,
        ago(skill.usage.last_used, report.generated_at),
    ));
    out.push_str(&format!("  source     {:?}\n", skill.source));
    out.push_str(&format!("  path       {}\n", skill.path.display()));
    out.push_str(&format!(
        "  tokens     always-on {} · on-fire {}\n",
        format_tokens(skill.cost.always_on),
        format_tokens(skill.cost.on_fire),
    ));
    if skill.disabled {
        out.push_str("  plugin     disabled via enabledPlugins (off — not loaded)\n");
    }
    if let Some(h) = &skill.history {
        out.push_str(&format!(
            "  typed      {}× (last {}){}\n",
            h.count,
            ago(h.last_used, report.generated_at),
            if skill.usage.count == 0 {
                " — absent from transcripts, likely rotated"
            } else {
                ""
            },
        ));
    }

    let inbound: Vec<&str> = report
        .edges
        .iter()
        .filter(|e| e.to == skill.name)
        .map(|e| e.from.as_str())
        .collect();
    let outbound: Vec<&str> = report
        .edges
        .iter()
        .filter(|e| e.from == skill.name)
        .map(|e| e.to.as_str())
        .collect();
    if !inbound.is_empty() {
        out.push_str(&format!("  linked from {}\n", inbound.join(", ")));
    }
    if !outbound.is_empty() {
        out.push_str(&format!("  links to    {}\n", outbound.join(", ")));
    }
    let mine: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.skill.as_deref() == Some(&skill.name))
        .collect();
    if !mine.is_empty() {
        out.push_str("\n  findings:\n");
        for f in mine {
            out.push_str(&format!("  ! [{}] {}\n", f.code, f.title));
            if let Some(fix) = &f.fix {
                out.push_str(&format!("    fix: {fix}\n"));
            }
        }
    }
    out
}

pub fn did_you_mean<'a>(name: &str, report: &'a Report) -> Option<&'a str> {
    report
        .skills
        .iter()
        .map(|s| (strsim::levenshtein(name, &s.name), s.name.as_str()))
        .filter(|(d, _)| *d <= 3)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n)
}
