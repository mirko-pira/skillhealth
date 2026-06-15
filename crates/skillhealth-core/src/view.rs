use serde::Serialize;
use std::path::PathBuf;

/// Which skills are listed. Project = this repo's only; User = user+plugin
/// (no project); All = everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Project,
    All,
    User,
}

/// Which transcripts feed usage heat. Project = sessions run inside the
/// project root; Global = all of them (v0.1 behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lens {
    Project,
    Global,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::All => "all",
            Scope::User => "user",
        }
    }

    /// `p`-key cycle: project → all → user → project. Without a project the
    /// Project stop is skipped.
    pub fn next(self, has_project: bool) -> Scope {
        match self {
            Scope::Project => Scope::All,
            Scope::All => Scope::User,
            Scope::User if has_project => Scope::Project,
            Scope::User => Scope::All,
        }
    }

    /// D2 (spec): the lens a scope change drags along.
    pub fn coupled_lens(self) -> Lens {
        match self {
            Scope::Project => Lens::Project,
            _ => Lens::Global,
        }
    }
}

impl Lens {
    pub fn label(self) -> &'static str {
        match self {
            Lens::Project => "project",
            Lens::Global => "global",
        }
    }

    pub fn toggled(self) -> Lens {
        match self {
            Lens::Project => Lens::Global,
            Lens::Global => Lens::Project,
        }
    }
}

/// The resolved view, embedded in every Report (additive in --json).
#[derive(Debug, Clone, Serialize)]
pub struct ViewInfo {
    pub scope: Scope,
    pub lens: Lens,
    pub project_root: Option<PathBuf>,
    /// file_name of project_root — shown as `scope: project (label)`.
    pub project_label: Option<String>,
    /// Where transcripts were looked for (fuels the empty-data warning).
    pub projects_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_cycle_skips_project_when_unavailable() {
        assert_eq!(Scope::Project.next(true), Scope::All);
        assert_eq!(Scope::All.next(true), Scope::User);
        assert_eq!(Scope::User.next(true), Scope::Project);
        assert_eq!(Scope::User.next(false), Scope::All);
    }

    #[test]
    fn coupled_lens_follows_scope() {
        assert_eq!(Scope::Project.coupled_lens(), Lens::Project);
        assert_eq!(Scope::All.coupled_lens(), Lens::Global);
        assert_eq!(Scope::User.coupled_lens(), Lens::Global);
    }

    #[test]
    fn serde_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&Scope::Project).unwrap(),
            "\"project\""
        );
        assert_eq!(serde_json::to_string(&Lens::Global).unwrap(), "\"global\"");
    }
}
