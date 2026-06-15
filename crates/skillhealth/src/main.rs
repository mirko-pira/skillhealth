#![forbid(unsafe_code)]

mod cli;
mod render;
mod scan;
mod tui;

use clap::Parser;
use cli::Cli;
use skillhealth_core::{cache, doctor};
use std::io::IsTerminal;

fn main() {
    let cli = Cli::parse();
    std::process::exit(run(cli));
}

fn run(cli: Cli) -> i32 {
    let ctx = scan::ScanContext::from_cli(&cli);

    let (auto_scope, auto_lens) = ctx.default_view();
    // A named detail lookup is an explicit ask — scope never hides it.
    let scope = if cli.name.is_some() {
        skillhealth_core::view::Scope::All
    } else {
        cli.scope.map(cli::ScopeArg::to_core).unwrap_or(auto_scope)
    };
    // D2 coupling: explicit --scope drags the lens unless --lens overrides.
    let lens = cli
        .lens
        .map(cli::LensArg::to_core)
        .unwrap_or(if cli.scope.is_some() {
            scope.coupled_lens()
        } else {
            auto_lens
        });

    // Bare `skillhealth` on an interactive stdout with no format flag → TUI.
    // The TUI owns its own scanning (splash + background refresh), so this
    // happens BEFORE the static pipeline scans anything. On init failure we
    // fall through and the static path below runs as if --plain was given.
    let wants_tui = cli.command.is_none()
        && cli.name.is_none()
        && !cli.json
        && !cli.md
        && !cli.plain
        && std::io::stdout().is_terminal();
    if wants_tui {
        // raw mode / alt screen denied → static fallback
        if let Ok(code) = tui::run(ctx.clone(), scope, lens) {
            return code;
        }
    }

    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();

    use indicatif::{ProgressBar, ProgressStyle};
    use std::sync::Mutex;
    let bar: Mutex<Option<ProgressBar>> = Mutex::new(None);
    let show_progress = std::io::stderr().is_terminal();
    let mut usage_cache = cache::load(&ctx.cache_path);
    let rep = ctx.scan_with(scope, lens, &mut usage_cache, &|done, total| {
        if !show_progress || total < 10 {
            return;
        }
        let mut guard = bar.lock().unwrap();
        let pb = guard.get_or_insert_with(|| {
            let pb = ProgressBar::new(total as u64);
            pb.set_style(
                ProgressStyle::with_template("scanning transcripts {bar:30} {pos}/{len}").unwrap(),
            );
            pb
        });
        pb.set_position(done as u64);
        if done == total {
            pb.finish_and_clear();
        }
    });

    // Friendly empty state: project scope outside any project (spec §1).
    if rep.view.scope == skillhealth_core::view::Scope::Project
        && rep.skills.is_empty()
        && cli.command.is_none()
        && cli.name.is_none()
    {
        if !cli.json && !cli.md {
            println!(
                "no project skills here (no .claude/skills under this repo) — try --scope all"
            );
        } else if cli.json {
            println!("{}", render::json::render_json(&rep));
        } else {
            print!("{}", render::markdown::render_markdown(&rep));
        }
        return 0;
    }

    match (&cli.command, &cli.name) {
        (None, None) => {
            if cli.json {
                println!("{}", render::json::render_json(&rep));
            } else if cli.md {
                print!("{}", render::markdown::render_markdown(&rep));
            } else {
                print!("{}", render::terminal::render_overview(&rep, color));
            }
            doctor::exit_code(&rep.findings)
        }
        (None, Some(name)) => {
            match rep
                .skills
                .iter()
                .find(|s| &s.name == name || s.name.ends_with(&format!(":{name}")))
            {
                Some(skill) => {
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(skill).unwrap());
                    } else {
                        print!("{}", render::detail::render_detail(&rep, skill));
                    }
                    0
                }
                None => {
                    match render::detail::did_you_mean(name, &rep) {
                        Some(sugg) => {
                            eprintln!("skill '{name}' not found — did you mean '{sugg}'?")
                        }
                        None => eprintln!("skill '{name}' not found"),
                    }
                    2
                }
            }
        }
        (Some(cli::Command::Doctor), _) => {
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&rep.findings).unwrap());
            } else {
                let flagged: std::collections::BTreeSet<_> = rep
                    .findings
                    .iter()
                    .filter_map(|f| f.skill.clone())
                    .collect();
                let clean = rep.skills.len().saturating_sub(flagged.len());
                print!(
                    "{}",
                    render::doctor_view::render_doctor(&rep.findings, clean, color)
                );
            }
            doctor::exit_code(&rep.findings)
        }
        (Some(cli::Command::Graph { open, format }), _) => match format {
            cli::GraphFormat::Mermaid => {
                print!("{}", render::markdown::render_mermaid(&rep));
                0
            }
            cli::GraphFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&rep.edges).unwrap());
                0
            }
            cli::GraphFormat::Html => match render::html::write_dashboard(&rep) {
                Ok(path) => {
                    println!("dashboard written to {}", path.display());
                    if *open {
                        let _ = open::that(&path);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("failed to write dashboard: {e}");
                    2
                }
            },
        },
    }
}
