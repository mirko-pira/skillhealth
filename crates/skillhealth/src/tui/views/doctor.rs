use crate::tui::app::App;
use crate::tui::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use skillhealth_core::model::Severity;

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let findings = app.sorted_findings();
    if findings.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "✓ all checks passed",
                Style::default().fg(theme.ok()).add_modifier(Modifier::BOLD),
            )))
            .block(Block::default().borders(Borders::ALL).title(" doctor ")),
            area,
        );
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let items: Vec<ListItem> = findings
        .iter()
        .map(|fi| {
            let (sym, color) = match fi.severity {
                Severity::Error => ("✗", theme.err()),
                Severity::Warn => ("!", theme.warn()),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{sym} "), Style::default().fg(color)),
                Span::styled(format!("[{}] ", fi.code), Style::default().fg(theme.dim())),
                Span::raw(fi.title.clone()),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.doctor_idx.min(findings.len() - 1)));
    f.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" findings "))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        cols[0],
        &mut state,
    );

    let fi = findings[app.doctor_idx.min(findings.len() - 1)];
    let (sev_label, sev_color) = match fi.severity {
        Severity::Error => ("error", theme.err()),
        Severity::Warn => ("warning", theme.warn()),
    };
    let detail_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(5)])
        .split(cols[1]);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                sev_label,
                Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  [{}]", fi.code), Style::default().fg(theme.dim())),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            fi.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::raw(format!("why: {}", fi.why)),
    ];
    if let Some(skill) = &fi.skill {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("skill: ", Style::default().fg(theme.dim())),
            Span::raw(skill.clone()),
            Span::styled("  (enter → jump to it)", Style::default().fg(theme.dim())),
        ]));
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" detail ")),
        detail_rows[0],
    );
    let fix_text = fi
        .fix
        .clone()
        .unwrap_or_else(|| "(no automated fix — read the why)".into());
    f.render_widget(
        Paragraph::new(fix_text).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" fix — y to copy "),
        ),
        detail_rows[1],
    );
}
