use crate::scan::ScanContext;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc::UnboundedSender;

/// Roots worth watching: user skills, plugins, project skills (walk-up),
/// and the transcripts dir. Only dirs that exist right now — notify errors
/// on missing paths.
pub fn watch_paths(ctx: &ScanContext) -> Vec<PathBuf> {
    let mut paths = vec![
        ctx.config_dir.join("skills"),
        ctx.config_dir.join("plugins"),
        ctx.projects_dir.clone(),
    ];
    paths.extend(skillhealth_core::discover::project_skill_dirs(
        &ctx.cwd,
        &ctx.config_dir,
    ));
    paths.retain(|p| p.is_dir());
    paths
}

/// Start watching. Every FS event becomes a unit ping on `tx` (the event
/// loop debounces and rescans — payloads don't matter, a change is a change).
/// Returns None when nothing could be watched → header shows "live off",
/// manual `r` refresh still works (spec error-handling table).
pub fn start(paths: &[PathBuf], tx: UnboundedSender<()>) -> Option<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })
    .ok()?;
    let mut watching = 0;
    for p in paths {
        if watcher.watch(p, RecursiveMode::Recursive).is_ok() {
            watching += 1;
        }
    }
    (watching > 0).then_some(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::ScanContext;

    fn ctx(root: &std::path::Path) -> ScanContext {
        ScanContext {
            config_dir: root.join("config"),
            projects_dir: root.join("projects"),
            cache_path: root.join("cache").join("usage-v1.json"),
            cwd: root.join("repo").join("sub"),
            now_override: None,
        }
    }

    #[test]
    fn watch_paths_includes_only_existing_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("config").join("skills")).unwrap();
        std::fs::create_dir_all(root.join("projects")).unwrap();
        std::fs::create_dir_all(root.join("repo").join(".claude").join("skills")).unwrap();
        std::fs::create_dir_all(root.join("repo").join("sub")).unwrap();
        // note: config/plugins does NOT exist → must be excluded

        let paths = watch_paths(&ctx(root));
        assert!(paths.contains(&root.join("config").join("skills")));
        assert!(paths.contains(&root.join("projects")));
        assert!(paths.contains(&root.join("repo").join(".claude").join("skills")));
        assert!(!paths.iter().any(|p| p.ends_with("plugins")));
    }

    #[test]
    fn watcher_starts_on_real_dirs_and_fails_gracefully_on_none() {
        let tmp = tempfile::tempdir().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(start(&[tmp.path().to_path_buf()], tx).is_some());
        let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel();
        assert!(start(&[], tx2).is_none()); // nothing to watch → live off
    }
}
