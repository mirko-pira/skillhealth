use assert_cmd::Command;

fn cmd() -> Command {
    let mut c = Command::cargo_bin("skillhealth").unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let cache = tempfile::tempdir().unwrap();
    // Use an isolated tempdir as cwd to prevent the project walk-up from finding
    // any real .claude/skills directories in the developer's home tree.
    let cwd = tempfile::tempdir().unwrap();
    c.arg("--config-dir").arg(fixtures.join("config"));
    c.arg("--projects-dir").arg(fixtures.join("projects"));
    c.arg("--cache-dir").arg(cache.keep());
    c.current_dir(cwd.keep());
    c.arg("--now").arg("2026-06-10T12:00:00Z");
    c
}

#[test]
fn overview_lists_skills_and_exits_nonzero_on_warnings() {
    let assert = cmd().assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("cfo"));
    assert!(out.contains("superpowers:writing-plans"));
    assert!(out.contains("old-experiment"));
    // debris + ghost-skill drift → warnings → exit 1
    assert.code(1);
}

#[test]
fn overview_mentions_doctor_when_findings_exist() {
    let assert = cmd().assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("skillhealth doctor"));
}

#[test]
fn json_output_is_valid_and_stable_schema() {
    let assert = cmd().arg("--json").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(v["schema_version"], 1);
    assert!(v["skills"].as_array().unwrap().len() >= 3);
    assert!(v["summary"]["total"].as_u64().unwrap() >= 3);
    assert!(v.get("findings").is_some());
    // privacy: no transcript content fields anywhere
    assert!(!out.contains("message"));
    // weekly buckets: additive field, 12 slots, sums never exceed count
    let cfo = v["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "cfo")
        .expect("cfo present");
    let weekly = cfo["weekly"].as_array().unwrap();
    assert_eq!(weekly.len(), 12);
    let sum: u64 = weekly.iter().map(|w| w.as_u64().unwrap()).sum();
    assert_eq!(sum, 2); // fixture fires cfo on 2026-06-08 + 2026-06-09, both in-window
}

#[test]
fn md_output_contains_mermaid_block() {
    let assert = cmd().arg("--md").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("```mermaid"));
    assert!(out.contains("| SKILL |") || out.contains("| skill |"));
}

#[test]
fn detail_view_shows_skill_info() {
    let assert = cmd().arg("cfo").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("cfo"));
    assert!(out.contains("Personal CFO advisor"));
    assert!(out.contains("hot"));
    assert!(out.contains("2 uses"));
}

#[test]
fn unknown_skill_suggests_and_exits_2() {
    let assert = cmd().arg("cfp").assert();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(err.contains("did you mean"));
    assert!(err.contains("cfo"));
    assert.code(2);
}

#[test]
fn doctor_lists_findings_with_fixes_and_exits_1_on_warnings() {
    let assert = cmd().arg("doctor").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("W003")); // debris
    assert!(out.contains("W007")); // ghost-skill drift
    assert!(out.contains("fix:"));
    assert!(out.contains("leftover-debris"));
    assert.code(1);
}

#[test]
fn doctor_json_mode() {
    let assert = cmd().arg("doctor").arg("--json").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v.as_array().unwrap().iter().any(|f| f["code"] == "W003"));
}

#[test]
fn graph_mermaid_format() {
    let assert = cmd().arg("graph").arg("--format").arg("mermaid").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.starts_with("graph TD"));
    assert!(out.contains("cfo"));
}

#[test]
fn graph_json_format_lists_edges() {
    let assert = cmd().arg("graph").arg("--format").arg("json").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v.as_array().is_some());
}

#[test]
fn graph_html_writes_dashboard_with_injected_data() {
    let assert = cmd().arg("graph").assert(); // default format = html, no --open
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    // prints the written file path
    let path = out.split_whitespace().last().unwrap().to_string();
    let html = std::fs::read_to_string(&path).unwrap();
    assert!(html.contains("\"schema_version\": 1") || html.contains("\"schema_version\":1"));
    assert!(!html.contains("/*__SKILLHEALTH_DATA__*/null"));
}

#[test]
fn plain_flag_forces_static_output_identical_to_piped_default() {
    let plain = cmd().arg("--plain").assert();
    let piped = cmd().assert();
    assert_eq!(
        String::from_utf8(plain.get_output().stdout.clone()).unwrap(),
        String::from_utf8(piped.get_output().stdout.clone()).unwrap()
    );
    plain.code(1); // fixture has warnings → static exit codes apply
}

#[test]
fn plain_flag_appears_in_help() {
    let assert = cmd().arg("--help").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("--plain"));
}

#[test]
fn scope_project_on_fixture_is_empty_friendly_exit_0() {
    // fixture cwd is an isolated tempdir → no project skills anywhere
    let assert = cmd().arg("--scope").arg("project").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("--scope all"));
    assert.code(0);
}

#[test]
fn scope_user_lists_and_json_carries_view_and_cost() {
    let assert = cmd().arg("--scope").arg("user").arg("--json").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["view"]["scope"], "user");
    assert_eq!(v["view"]["lens"], "global");
    let cfo = v["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "cfo")
        .unwrap();
    assert!(cfo["cost"]["always_on"].as_u64().is_some());
    assert!(cfo["cost"]["on_fire"].as_u64().is_some());
    assert!(v["summary"]["always_on_total"].as_u64().is_some());
}

#[test]
fn static_header_shows_scope_and_lens() {
    let assert = cmd().assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("scope: all"), "got:\n{out}");
    assert!(out.contains("lens: global"));
    assert!(out.contains("always-on total:"));
}

#[test]
fn doctor_w010_fires_for_typed_but_rotated_skill() {
    let assert = cmd().arg("doctor").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("W010"), "got:\n{out}");
    assert!(out.contains("old-experiment"));
    assert!(
        !out.lines()
            .any(|l| l.contains("W010") && l.contains("ghost-rotated")),
        "uninstalled history entry must not fire W010 — got:\n{out}"
    );
}

#[test]
fn detail_shows_typed_history_line() {
    let assert = cmd().arg("old-experiment").assert();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("typed"));
    assert!(out.contains("absent from transcripts"));
}
