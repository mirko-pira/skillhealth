use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

pub const EXAMPLES: &str = "\
EXAMPLES:
  skillhealth                 overview of all skills (hot/warm/cold/dead)
  skillhealth cfo             detail for the 'cfo' skill
  skillhealth doctor          diagnostics with copy-pasteable fixes
  skillhealth graph --open    interactive HTML dashboard in the browser
  skillhealth --json          overview as JSON (exit codes: 0 ok, 1 warnings, 2 errors)";

#[derive(Parser)]
#[command(name = "skillhealth", version, about = "Audit & observability for your agent skills", after_help = EXAMPLES)]
pub struct Cli {
    /// Show details for one skill
    pub name: Option<String>,

    /// Machine-readable JSON output
    #[arg(long, global = true)]
    pub json: bool,

    /// Markdown report (includes a Mermaid graph)
    #[arg(long, global = true, conflicts_with = "json")]
    pub md: bool,

    /// Force the plain static report even on an interactive terminal
    #[arg(long, global = true)]
    pub plain: bool,

    /// Which skills are listed: this repo's, everything, or user+plugins only
    #[arg(long, global = true, value_enum)]
    pub scope: Option<ScopeArg>,

    /// Which transcripts feed usage heat: this project's only, or all of them
    #[arg(long, global = true, value_enum)]
    pub lens: Option<LensArg>,

    /// Claude config root (default: $CLAUDE_CONFIG_DIR, else ~/.claude)
    #[arg(long, global = true)]
    pub config_dir: Option<PathBuf>,
    /// Transcripts root for usage heat (default: ~/.claude/projects —
    /// independent of --config-dir)
    #[arg(long, global = true)]
    pub projects_dir: Option<PathBuf>,
    /// Cache directory (default: the platform cache dir)
    #[arg(long, global = true)]
    pub cache_dir: Option<PathBuf>,
    #[arg(long, global = true, hide = true, value_parser = parse_now)]
    pub now: Option<DateTime<Utc>>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Diagnose problems and print copy-pasteable fixes
    Doctor,
    /// Skill relationship graph
    Graph {
        /// Open the HTML dashboard in the browser
        #[arg(long)]
        open: bool,
        /// Output format
        #[arg(long, value_enum, default_value_t = GraphFormat::Html)]
        format: GraphFormat,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum GraphFormat {
    Html,
    Mermaid,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ScopeArg {
    Project,
    All,
    User,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum LensArg {
    Project,
    Global,
}

impl ScopeArg {
    pub fn to_core(self) -> skillhealth_core::view::Scope {
        match self {
            ScopeArg::Project => skillhealth_core::view::Scope::Project,
            ScopeArg::All => skillhealth_core::view::Scope::All,
            ScopeArg::User => skillhealth_core::view::Scope::User,
        }
    }
}

impl LensArg {
    pub fn to_core(self) -> skillhealth_core::view::Lens {
        match self {
            LensArg::Project => skillhealth_core::view::Lens::Project,
            LensArg::Global => skillhealth_core::view::Lens::Global,
        }
    }
}

fn parse_now(s: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| e.to_string())
}
