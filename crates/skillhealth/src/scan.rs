use chrono::{DateTime, Utc};
use skillhealth_core::cache::UsageCache;
use skillhealth_core::report::Report;
use skillhealth_core::view::{Lens, Scope, ViewInfo};
use skillhealth_core::{
    cache, claude_md, discover, doctor, graph, history, report, settings, usage,
};
use std::path::PathBuf;

/// Everything a scan needs, resolved once from CLI/env. Cloneable so the
/// TUI can hand it to background rescans.
#[derive(Clone)]
pub struct ScanContext {
    pub config_dir: PathBuf,
    pub projects_dir: PathBuf,
    pub cache_path: PathBuf,
    pub cwd: PathBuf,
    /// Some(..) when --now is pinned (tests, demo tape); None = wall clock.
    pub now_override: Option<DateTime<Utc>>,
}

impl ScanContext {
    pub fn from_cli(cli: &crate::cli::Cli) -> Self {
        let config_dir = cli
            .config_dir
            .clone()
            .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"));
        let projects_dir = resolve_projects_dir(
            cli.projects_dir.clone(),
            std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from),
            dirs::home_dir(),
        );
        let cache_path = cli
            .cache_dir
            .clone()
            .unwrap_or_else(|| {
                dirs::cache_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("skillhealth")
            })
            .join("usage-v1.json");
        ScanContext {
            config_dir,
            projects_dir,
            cache_path,
            cwd: std::env::current_dir().unwrap_or_default(),
            now_override: cli.now,
        }
    }

    /// Live refreshes use the wall clock unless --now pins it.
    pub fn now(&self) -> DateTime<Utc> {
        self.now_override.unwrap_or_else(Utc::now)
    }

    pub fn project_root(&self) -> Option<PathBuf> {
        discover::project_root(&self.cwd, &self.config_dir)
    }

    /// D3 auto-detect: project skills under cwd → (Project, Project) coupled;
    /// otherwise v0.1 behavior (All, Global).
    pub fn default_view(&self) -> (Scope, Lens) {
        if discover::project_skill_dirs(&self.cwd, &self.config_dir).is_empty() {
            (Scope::All, Lens::Global)
        } else {
            (Scope::Project, Lens::Project)
        }
    }

    /// Full pipeline: auto-detects scope/lens from cwd, then calls scan_with.
    #[allow(dead_code)]
    pub fn scan(
        &self,
        usage_cache: &mut UsageCache,
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Report {
        let (scope, lens) = self.default_view();
        self.scan_with(scope, lens, usage_cache, on_progress)
    }

    /// Full pipeline with an explicit view: discover → disabled-marking →
    /// usage (lens-filtered) → history → CLAUDE.md → graph → doctor → report.
    pub fn scan_with(
        &self,
        scope: Scope,
        lens: Lens,
        usage_cache: &mut UsageCache,
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Report {
        let now = self.now();
        let project_root = self.project_root();
        // Project lens without a project degrades to Global (header shows it).
        let lens = if lens == Lens::Project && project_root.is_none() {
            Lens::Global
        } else {
            lens
        };
        let lens_root = if lens == Lens::Project {
            project_root.clone()
        } else {
            None
        };
        let mut discovery = discover::discover(&discover::DiscoverInput {
            config_dir: self.config_dir.clone(),
            cwd: self.cwd.clone(),
        });
        let disabled = settings::disabled_plugins(&self.config_dir, project_root.as_deref());
        discovery.apply_disabled(&disabled);
        let scan = usage::scan_usage(
            &self.projects_dir,
            usage_cache,
            lens_root.as_deref(),
            on_progress,
        );
        let _ = cache::save(&self.cache_path, usage_cache);
        let raw_history =
            history::scan_history(&self.config_dir.join("history.jsonl"), lens_root.as_deref());
        let history = history::match_to_skills(&discovery.skills, &raw_history);
        let mds = claude_md::collect(&self.config_dir, &self.cwd);
        let md_pairs: Vec<(String, String)> = mds
            .iter()
            .map(|m| (m.path.display().to_string(), m.content.clone()))
            .collect();
        let edges = graph::build_graph(&discovery.skills, &md_pairs);
        let findings = doctor::run(&discovery, &scan, &mds, &history, now);
        let view = ViewInfo {
            scope,
            lens,
            project_label: project_root
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            project_root,
            projects_dir: self.projects_dir.clone(),
        };
        report::build(&discovery, &scan, edges, findings, &history, view, now)
    }
}

/// D7 (spec): an explicit --config-dir FLAG no longer drags the transcripts
/// root with it (that silently zeroed all heat). CLAUDE_CONFIG_DIR env still
/// moves everything together.
pub fn resolve_projects_dir(
    flag: Option<PathBuf>,
    env_config: Option<PathBuf>,
    home: Option<PathBuf>,
) -> PathBuf {
    flag.or_else(|| env_config.map(|d| d.join("projects")))
        .unwrap_or_else(|| home.unwrap_or_default().join(".claude").join("projects"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_dir_resolution_flag_env_home() {
        // D7: explicit flag > CLAUDE_CONFIG_DIR env > home — a --config-dir
        // FLAG override must NOT drag projects_dir anymore.
        assert_eq!(
            resolve_projects_dir(
                Some(PathBuf::from("/x")),
                Some(PathBuf::from("/env")),
                Some(PathBuf::from("/home"))
            ),
            PathBuf::from("/x")
        );
        assert_eq!(
            resolve_projects_dir(
                None,
                Some(PathBuf::from("/env")),
                Some(PathBuf::from("/home"))
            ),
            PathBuf::from("/env/projects")
        );
        assert_eq!(
            resolve_projects_dir(None, None, Some(PathBuf::from("/home"))),
            PathBuf::from("/home/.claude/projects")
        );
    }

    #[test]
    fn default_view_is_project_inside_a_skills_repo_else_all() {
        use skillhealth_core::view::{Lens, Scope};
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(".claude").join("skills")).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let ctx = ScanContext {
            config_dir: tmp.path().join("claude"),
            projects_dir: tmp.path().join("projects"),
            cache_path: tmp.path().join("cache.json"),
            cwd: repo.clone(),
            now_override: None,
        };
        assert_eq!(ctx.default_view(), (Scope::Project, Lens::Project));
        let outside = ScanContext {
            cwd: tmp.path().to_path_buf(),
            ..ctx
        };
        assert_eq!(outside.default_view(), (Scope::All, Lens::Global));
    }
}
