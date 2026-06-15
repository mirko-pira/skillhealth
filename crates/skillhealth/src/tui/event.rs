use crate::scan::ScanContext;
use crate::tui::app::{Action, App};
use crate::tui::debounce::Debouncer;
use crate::tui::theme::Theme;
use crate::tui::{actions, ui, watch};
use futures_util::StreamExt;
use ratatui::DefaultTerminal;
use skillhealth_core::cache;
use skillhealth_core::cache::UsageCache;
use skillhealth_core::report::Report;
use skillhealth_core::view::{Lens, Scope};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

pub enum AppEvent {
    Report(Box<Report>, u64),
    /// Rescan blew up: keep the last good report, toast the failure (spec).
    ScanFailed(String, u64), // generation tag — matches scan_gen in event_loop
}

pub fn run_loop(
    terminal: DefaultTerminal,
    ctx: ScanContext,
    scope: Scope,
    lens: Lens,
) -> anyhow::Result<i32> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(event_loop(terminal, ctx, scope, lens))
}

fn spawn_scan(
    ctx: &ScanContext,
    view: (Scope, Lens),
    cache: &Arc<Mutex<UsageCache>>,
    tx: &UnboundedSender<AppEvent>,
    scan_id: u64,
) {
    let ctx = ctx.clone();
    let cache = Arc::clone(cache);
    let tx = tx.clone();
    tokio::task::spawn_blocking(move || {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut guard = cache.lock().unwrap();
            ctx.scan_with(view.0, view.1, &mut guard, &|_, _| {})
        }));
        let _ = match result {
            Ok(report) => tx.send(AppEvent::Report(Box::new(report), scan_id)),
            Err(_) => tx.send(AppEvent::ScanFailed(
                "rescan failed — showing last good report".into(),
                scan_id,
            )),
        };
    });
}

async fn event_loop(
    mut terminal: DefaultTerminal,
    ctx: ScanContext,
    scope: Scope,
    lens: Lens,
) -> anyhow::Result<i32> {
    let theme = Theme::from_env();
    let mut app = App::new();
    let (tx, mut rx) = unbounded_channel::<AppEvent>();
    let (wtx, mut wrx) = unbounded_channel::<()>();

    let usage_cache = Arc::new(Mutex::new(cache::load(&ctx.cache_path)));
    // Watcher handle must stay alive for the whole loop — dropping it stops events.
    let _watcher = watch::start(&watch::watch_paths(&ctx), wtx);
    app.live = _watcher.is_some();

    let mut view = (scope, lens);
    // Finding 2: seed the app's display state from the CLI args so the header
    // shows the correct scope/lens before the first report arrives.
    app.scope = scope;
    app.lens = lens;

    // Generation counter: bumped on every spawn_scan so the event handler can
    // discard reports from superseded scans (latest-wins).
    let mut scan_gen: u64 = 0;
    app.scanning = true;
    spawn_scan(&ctx, view, &usage_cache, &tx, scan_gen);

    let mut events = crossterm::event::EventStream::new();
    // 120ms tick: drives spinner/pulse AND polls the debounce deadline, so a
    // debounced rescan fires at most ~620ms after the last FS event.
    let mut tick = tokio::time::interval(Duration::from_millis(120));
    let mut debounce = Debouncer::new(Duration::from_millis(500));

    loop {
        terminal.draw(|f| ui::draw(f, &app, &theme))?;

        let mut pending: Option<Action> = None;
        tokio::select! {
            _ = tick.tick() => {
                app.tick = app.tick.wrapping_add(1);
                if app.flash_frames > 0 {
                    app.flash_frames -= 1;
                }
                if debounce.should_fire(Instant::now()) && !app.scanning {
                    app.toast = None;
                    app.scanning = true;
                    scan_gen = scan_gen.wrapping_add(1);
                    spawn_scan(&ctx, view, &usage_cache, &tx, scan_gen);
                }
            }
            Some(Ok(ev)) = events.next() => {
                if let crossterm::event::Event::Key(key) = ev
                    && key.kind == crossterm::event::KeyEventKind::Press
                {
                    pending = app.on_key(key);
                }
            }
            Some(()) = wrx.recv() => {
                debounce.on_event(Instant::now());
            }
            Some(ev) = rx.recv() => match ev {
                AppEvent::Report(r, id) if id == scan_gen => app.apply_report(*r),
                AppEvent::ScanFailed(msg, id) if id == scan_gen => {
                    app.scanning = false;
                    app.toast = Some(msg);
                }
                // Stale report from a superseded scan — discard silently.
                _ => {}
            }
        }

        match pending {
            Some(Action::Quit) => return Ok(0),
            Some(Action::Refresh) if !app.scanning => {
                app.toast = None;
                app.scanning = true;
                scan_gen = scan_gen.wrapping_add(1);
                spawn_scan(&ctx, view, &usage_cache, &tx, scan_gen);
            }
            Some(Action::Refresh) => {}
            Some(Action::SetView(s, l)) => {
                view = (s, l);
                app.toast = None;
                app.scanning = true;
                scan_gen = scan_gen.wrapping_add(1);
                spawn_scan(&ctx, view, &usage_cache, &tx, scan_gen);
            }
            Some(Action::OpenEditor(path)) => {
                // The EventStream polls stdin from another task — it MUST be
                // gone while the editor owns the terminal, then rebuilt.
                drop(events);
                if let Err(e) = actions::open_editor(&mut terminal, &path) {
                    app.toast = Some(format!("editor: {e}"));
                }
                events = crossterm::event::EventStream::new();
                terminal.clear()?;
            }
            Some(Action::OpenGraph) => match app.report.as_ref() {
                Some(report) => match actions::open_graph(report) {
                    Ok(()) => app.toast = Some("dashboard opened in browser".into()),
                    Err(e) => app.toast = Some(format!("graph: {e}")),
                },
                None => app.toast = Some("no report yet".into()),
            },
            Some(Action::CopyFix(fix)) => {
                actions::copy_to_clipboard(&fix);
                app.toast = Some("fix copied to clipboard".into());
            }
            None => {}
        }
    }
}
