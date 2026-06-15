use crate::tui::app::{App, View};
use crate::tui::theme::{Theme, gradient_spans};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(f: &mut Frame, app: &App, theme: &Theme) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());
    draw_header(f, rows[0], app, theme);
    if app.report.is_none() {
        draw_splash(f, rows[1], app, theme);
    } else {
        match app.view {
            View::Overview => crate::tui::views::overview::render(f, rows[1], app, theme),
            View::Doctor => crate::tui::views::doctor::render(f, rows[1], app, theme),
        }
    }
    draw_status(f, rows[2], app, theme);
    if app.show_help {
        crate::tui::views::help::render(f, rows[1]);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mut spans = gradient_spans("skillhealth", theme);
    if let Some(r) = &app.report {
        let s = &r.summary;
        spans.push(Span::raw(format!(
            " · {} skills · {} hot · {} warm · {} cold · {} dead",
            s.total, s.hot, s.warm, s.cold, s.dead
        )));
        if app.view == View::Doctor {
            spans.push(Span::styled(
                format!("  {}E {}W", s.errors, s.warnings),
                Style::default()
                    .fg(theme.err())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let project = app
            .project_label
            .as_deref()
            .map(|l| format!(" ({l})"))
            .unwrap_or_default();
        spans.push(Span::styled(
            format!(
                " · scope: {}{} · lens: {}",
                app.scope.label(),
                project,
                app.lens.label()
            ),
            Style::default().fg(theme.dim()),
        ));
    }
    if app.scanning {
        spans.push(Span::styled(
            format!(" {} scanning", SPINNER[(app.tick as usize) % SPINNER.len()]),
            Style::default().fg(theme.warn()),
        ));
    }
    if app.flash_frames > 0 {
        spans.push(Span::styled(
            " · updated",
            Style::default().fg(theme.ok()).add_modifier(Modifier::BOLD),
        ));
    }
    // right side: pulsing live dot + last refresh
    let pulse = app.tick % 6 < 3;
    let dot_style = if app.live && pulse {
        Style::default().fg(theme.ok())
    } else if app.live {
        Style::default().fg(theme.dim())
    } else {
        Style::default().fg(theme.err())
    };
    let live_label = if app.live { "● live" } else { "● live off" };
    let stamp = app
        .report
        .as_ref()
        .map(|r| r.generated_at.format(" · %H:%M:%S").to_string())
        .unwrap_or_default();
    let right = Line::from(vec![
        Span::styled(live_label, dot_style),
        Span::styled(stamp, Style::default().fg(theme.dim())),
    ])
    .right_aligned();
    f.render_widget(Paragraph::new(Line::from(spans)), area);
    f.render_widget(Paragraph::new(right), area);
}

fn draw_splash(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let spinner = SPINNER[(app.tick as usize) % SPINNER.len()];
    let line = Line::from(vec![
        Span::styled(spinner.to_string(), Style::default().fg(theme.warn())),
        Span::raw(" scanning skills and transcripts…"),
    ])
    .centered();
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);
    f.render_widget(Paragraph::new(line), v[1]);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let text: Line = if app.filter_input {
        Line::from(vec![
            Span::styled("filter: ", Style::default().fg(theme.dim())),
            Span::raw(app.filter.clone()),
            Span::styled("▌", Style::default().fg(theme.ok())),
            Span::styled(
                "  esc clear · enter apply",
                Style::default().fg(theme.dim()),
            ),
        ])
    } else if let Some(toast) = &app.toast {
        Line::from(Span::styled(
            toast.clone(),
            Style::default().fg(theme.warn()),
        ))
    } else {
        let hints = match app.view {
            View::Overview => {
                " j/k move · / filter · p scope · L lens · s sort · S group · enter open · d doctor · g graph · r refresh · ? help · q quit"
            }
            View::Doctor => {
                " j/k move · enter goto skill · y copy fix · d overview · r refresh · ? help · q quit"
            }
        };
        Line::from(Span::styled(hints, Style::default().fg(theme.dim())))
    };
    f.render_widget(Paragraph::new(text), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::{App, View};
    use crate::tui::fixtures::fixture_report;
    use crate::tui::theme::Theme;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_text(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|f| draw(f, app, &Theme { color: false }))
            .unwrap();
        let buf = terminal.backend().buffer();
        let area = *buf.area();
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn splash_renders_while_no_report() {
        let mut app = App::new();
        app.scanning = true;
        let text = render_text(&app);
        assert!(text.contains("scanning"));
        assert!(text.contains("skillhealth"));
    }

    #[test]
    fn overview_lists_skills_with_glyphs_and_detail_pane() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        let text = render_text(&app);
        assert!(text.contains("cfo"));
        assert!(text.contains("old-experiment"));
        assert!(text.contains("◌")); // dead glyph
        assert!(text.contains("description of cfo")); // detail of selection
        assert!(text.contains("4 skills"));
        assert!(text.contains("1200") || text.contains("1.2k")); // est tokens
        assert!(text.contains("finance")); // edge target listed in detail
    }

    #[test]
    fn empty_filter_state_says_no_match() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app.filter = "zzzz".into();
        let text = render_text(&app);
        assert!(text.contains("no skills match"));
    }

    #[test]
    fn header_shows_live_off_and_last_refresh() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app.live = false;
        let text = render_text(&app);
        assert!(text.contains("live off"));
        assert!(text.contains("12:00")); // generated_at stamp
    }

    #[test]
    fn status_bar_shows_filter_input_mode() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app.filter_input = true;
        app.filter = "cf".into();
        let text = render_text(&app);
        assert!(text.contains("filter: cf"));
    }

    #[test]
    fn status_bar_shows_toast_over_hints() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app.toast = Some("editor failed".into());
        let text = render_text(&app);
        assert!(text.contains("editor failed"));
    }

    #[test]
    fn doctor_view_dispatches() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app.view = View::Doctor;
        let text = render_text(&app);
        assert!(text.contains("E001"));
        assert!(text.contains("1E"));
        assert!(text.contains("1W"));
    }

    #[test]
    fn doctor_shows_fix_block_and_copy_hint() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app.view = View::Doctor;
        let text = render_text(&app);
        assert!(text.contains("$EDITOR")); // the fix command
        assert!(text.contains("y to copy"));
        assert!(text.contains("broken frontmatter"));
    }

    #[test]
    fn doctor_with_no_findings_says_all_clear() {
        let mut app = App::new();
        let mut report = fixture_report();
        report.findings.clear();
        report.summary.errors = 0;
        report.summary.warnings = 0;
        app.apply_report(report);
        app.view = View::Doctor;
        let text = render_text(&app);
        assert!(text.contains("all checks passed"));
    }

    #[test]
    fn help_overlay_lists_keymap() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app.show_help = true;
        let text = render_text(&app);
        assert!(text.contains("keymap"));
        assert!(text.contains("open in $EDITOR"));
        assert!(text.contains("toggle doctor"));
    }

    fn render_text_wide(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(160, 30)).unwrap();
        terminal
            .draw(|f| draw(f, app, &Theme { color: false }))
            .unwrap();
        let buf = terminal.backend().buffer();
        let area = *buf.area();
        let mut out = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn header_shows_scope_and_lens() {
        let mut app = App::new();
        let mut r = fixture_report();
        r.view.scope = skillhealth_core::view::Scope::Project;
        r.view.lens = skillhealth_core::view::Lens::Project;
        r.view.project_root = Some(std::path::PathBuf::from("/dev/demo-app"));
        r.view.project_label = Some("demo-app".into());
        app.apply_report(r);
        let text = render_text_wide(&app);
        assert!(text.contains("scope: project (demo-app)"), "got:\n{text}");
        assert!(text.contains("lens: project"));
    }

    #[test]
    fn hints_include_scope_and_lens_keys() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        let text = render_text(&app);
        assert!(text.contains("p scope"));
        assert!(text.contains("L lens"));
    }

    #[test]
    fn disabled_skill_shows_off_badge_and_detail_explains() {
        let mut app = App::new();
        let mut r = fixture_report();
        r.skills[2].disabled = true; // superpowers:writing-plans
        app.apply_report(r);
        app.selected = Some("superpowers:writing-plans".into());
        let text = render_text(&app);
        // "○ " prefix is the disabled-row glyph; "   2 off" pins count(=2)+badge as rendered
        // by overview.rs `format!("{:>4} ", count)` + "off" span.
        // The bare `contains("off")` was vacuous: "● live off" in the header always matched.
        assert!(
            text.contains("○ superpowers:writing-plans"),
            "disabled row ○ prefix missing — got:\n{text}"
        );
        assert!(
            text.contains("   2 off"),
            "disabled badge count+off missing — got:\n{text}"
        );
        assert!(text.contains("disabled via enabledPlugins"));
    }

    #[test]
    fn detail_shows_cost_split_and_typed_history() {
        let mut app = App::new();
        let mut r = fixture_report();
        r.skills[0].history = Some(skillhealth_core::history::HistoryStats {
            count: 26,
            last_used: None,
        });
        app.apply_report(r);
        let text = render_text(&app);
        assert!(text.contains("always-on"));
        assert!(text.contains("on-fire"));
        assert!(text.contains("typed"));
        assert!(text.contains("26"));
    }

    #[test]
    fn help_overlay_lists_scope_and_lens() {
        let mut app = App::new();
        app.apply_report(fixture_report());
        app.show_help = true;
        let text = render_text(&app);
        assert!(text.contains("cycle scope"));
        assert!(text.contains("toggle lens"));
    }
}
