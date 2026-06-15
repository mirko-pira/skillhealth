mod actions;
mod app;
mod debounce;
mod event;
#[cfg(test)]
mod fixtures;
mod theme;
mod ui;
mod views;
mod watch;

use crate::scan::ScanContext;
use skillhealth_core::view::{Lens, Scope};

/// Run the live cockpit. Returns the process exit code (0 on clean quit).
/// An Err means "this terminal can't do TUI" (raw mode / alt screen denied)
/// — the caller falls back to the static overview. We fail fast BEFORE any
/// scanning so the fallback never double-scans.
pub fn run(ctx: ScanContext, scope: Scope, lens: Lens) -> anyhow::Result<i32> {
    // Panic hook: restore the terminal before the default hook prints, so a
    // panicking TUI never leaves the shell in raw mode (spec requirement).
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        prev_hook(info);
    }));

    let terminal = ratatui::try_init()?;
    let result = event::run_loop(terminal, ctx, scope, lens);
    ratatui::restore();
    let _ = std::panic::take_hook(); // drop our hook once the terminal is back
    result
}
