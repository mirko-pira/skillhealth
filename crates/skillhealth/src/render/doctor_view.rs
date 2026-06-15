use owo_colors::OwoColorize;
use skillhealth_core::doctor::Finding;
use skillhealth_core::model::Severity;

pub fn render_doctor(findings: &[Finding], checks_ok: usize, color: bool) -> String {
    let mut out = String::from("skillhealth doctor\n\n");
    if findings.is_empty() {
        out.push_str("✓ all checks passed\n");
        return out;
    }
    let mut sorted: Vec<_> = findings.iter().collect();
    sorted.sort_by_key(|f| match f.severity {
        Severity::Error => 0,
        Severity::Warn => 1,
    });
    for f in sorted {
        let sym = match (f.severity, color) {
            (Severity::Error, true) => "✗".red().to_string(),
            (Severity::Error, false) => "✗".to_string(),
            (Severity::Warn, true) => "!".yellow().to_string(),
            (Severity::Warn, false) => "!".to_string(),
        };
        out.push_str(&format!("{sym} [{}] {}\n", f.code, f.title));
        out.push_str(&format!("    why: {}\n", f.why));
        if let Some(fix) = &f.fix {
            out.push_str(&format!("    fix: {fix}\n"));
        }
        out.push('\n');
    }
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warns = findings.len() - errors;
    out.push_str(&format!(
        "{errors} error(s), {warns} warning(s), {checks_ok} skill(s) clean\n"
    ));
    out
}
