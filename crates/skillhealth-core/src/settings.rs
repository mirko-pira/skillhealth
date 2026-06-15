use std::collections::BTreeSet;
use std::path::Path;

/// Plugins explicitly switched off via `enabledPlugins`. Read order: user
/// `<config>/settings.json`, then `<project>/.claude/settings.json`, then
/// `<project>/.claude/settings.local.json` — later files override earlier
/// ones per key. Only an explicit `false` disables; missing = enabled.
/// Keys are `plugin@marketplace`; the returned set holds the plugin half.
pub fn disabled_plugins(config_dir: &Path, project_root: Option<&Path>) -> BTreeSet<String> {
    let mut state: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut paths = vec![config_dir.join("settings.json")];
    if let Some(root) = project_root {
        paths.push(root.join(".claude").join("settings.json"));
        paths.push(root.join(".claude").join("settings.local.json"));
    }
    for p in paths {
        let Ok(raw) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(map) = v.get("enabledPlugins").and_then(|m| m.as_object()) else {
            continue;
        };
        for (k, val) in map {
            if let Some(b) = val.as_bool() {
                state.insert(k.clone(), b);
            }
        }
    }
    state
        .into_iter()
        .filter(|(_, enabled)| !enabled)
        .map(|(k, _)| k.split('@').next().unwrap_or("").to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &std::path::Path, content: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn explicit_false_disables_project_overrides_user() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        let proj = tmp.path().join("repo");
        write(
            &config.join("settings.json"),
            r#"{"enabledPlugins":{"cartographer@mkt":false,"superpowers@official":true}}"#,
        );
        write(
            &proj.join(".claude").join("settings.json"),
            r#"{"enabledPlugins":{"superpowers@official":false}}"#,
        );
        write(
            &proj.join(".claude").join("settings.local.json"),
            r#"{"enabledPlugins":{"cartographer@mkt":true}}"#,
        );
        let off = disabled_plugins(&config, Some(&proj));
        // local re-enabled cartographer; project disabled superpowers
        assert!(!off.contains("cartographer"));
        assert!(off.contains("superpowers"));
    }

    #[test]
    fn missing_files_and_keys_mean_enabled() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(disabled_plugins(&tmp.path().join("nope"), None).is_empty());
        let config = tmp.path().join("claude");
        write(&config.join("settings.json"), r#"{"theme":"dark"}"#);
        assert!(disabled_plugins(&config, None).is_empty());
    }

    #[test]
    fn malformed_json_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("claude");
        write(&config.join("settings.json"), "{not json");
        assert!(disabled_plugins(&config, None).is_empty());
    }
}
