use skillhealth_core::report::Report;
use std::path::PathBuf;

const DASHBOARD_TEMPLATE: &str = include_str!("../../assets/dashboard.html");
const PLACEHOLDER: &str = "/*__SKILLHEALTH_DATA__*/null";

/// Escape JSON for safe embedding inside an HTML `<script>` block.
///
/// Standard technique: replace `</` with `<\/` so a skill description
/// containing `</script>` cannot break out of the enclosing script element.
/// We also escape `<!--` → `<\!--` to prevent comment-injection.
fn escape_json_for_script(json: &str) -> String {
    json.replace("</", r"<\/").replace("<!--", r"<\!--")
}

pub fn render_html(report: &Report) -> String {
    let raw = serde_json::to_string_pretty(report).expect("report serializes");
    let data = escape_json_for_script(&raw);
    DASHBOARD_TEMPLATE.replace(PLACEHOLDER, &data)
}

pub fn write_dashboard(report: &Report) -> std::io::Result<PathBuf> {
    let path = std::env::temp_dir().join("skillhealth-dashboard.html");
    std::fs::write(&path, render_html(report))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use skillhealth_core::model::{SkillSource, Temperature, UsageStats};
    use skillhealth_core::report::{Cost, Report, ReportSkill, Summary};
    use std::path::PathBuf;

    fn report_with_description(desc: &str) -> Report {
        Report {
            schema_version: 1,
            generated_at: Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap(),
            summary: Summary {
                total: 1,
                hot: 1,
                ..Default::default()
            },
            skills: vec![ReportSkill {
                name: "xss-test".to_string(),
                source: SkillSource::User,
                path: PathBuf::from("/fake/SKILL.md"),
                description: Some(desc.to_string()),
                temperature: Temperature::Hot,
                usage: UsageStats::default(),
                est_tokens: 10,
                cost: Cost {
                    always_on: 1,
                    on_fire: 10,
                },
                disabled: false,
                history: None,
                weekly: [0; 12],
            }],
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
    fn script_close_tag_in_description_is_escaped() {
        let report = report_with_description("</script><script>alert(1)</script>");
        let html = render_html(&report);

        // The data section must NOT contain a raw closing </script> tag from the data.
        // The template itself has its own closing tags; we count those and verify the
        // data-injected ones are escaped as <\/script>.
        assert!(
            html.contains(r"<\/script>"),
            "expected escaped <\\/script> in data section"
        );
        // Verify no raw </script> appears inside the JSON data blob.
        // The placeholder is between the opening <script> and the first </script>,
        // so we extract that region and check it has no unescaped </script>.
        let script_start = html.find("window.__SKILLHEALTH_DATA__").unwrap();
        let script_end = html[script_start..].find("</script>").unwrap();
        let data_region = &html[script_start..script_start + script_end];
        assert!(
            !data_region.contains("</script>"),
            "raw </script> found in data region — HTML injection not properly escaped"
        );
    }

    #[test]
    fn html_comment_injection_is_escaped() {
        let report = report_with_description("<!-- inject");
        let html = render_html(&report);
        // Find the data region only (between the script opener and first </script>)
        let script_start = html.find("window.__SKILLHEALTH_DATA__").unwrap();
        let script_end = html[script_start..].find("</script>").unwrap();
        let data_region = &html[script_start..script_start + script_end];
        assert!(
            !data_region.contains("<!--"),
            "raw <!-- should be escaped in data region"
        );
        assert!(html.contains(r"<\!--"));
    }

    #[test]
    fn normal_description_survives_escape_unchanged() {
        let report = report_with_description("Personal CFO advisor for spending decisions");
        let html = render_html(&report);
        assert!(html.contains("Personal CFO advisor for spending decisions"));
    }
}
