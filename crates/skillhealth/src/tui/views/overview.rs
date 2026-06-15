use crate::render::terminal::{ago, format_tokens};
use crate::tui::app::App;
use crate::tui::theme::{Theme, glyph};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Sparkline, Wrap};
use skillhealth_core::graph::EdgeKind;
use skillhealth_core::model::{Severity, SkillSource};

/// 12-char mini-sparkline for list rows. Zero weeks stay blank so dead
/// stretches read as silence, max scales to the tallest block.
pub fn spark(weekly: &[u32; 12]) -> String {
    const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = weekly.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return " ".repeat(12);
    }
    weekly
        .iter()
        .map(|&v| {
            if v == 0 {
                ' '
            } else {
                LEVELS[(v as usize * (LEVELS.len() - 1) / max as usize).min(LEVELS.len() - 1)]
            }
        })
        .collect()
}

fn source_label(s: &SkillSource) -> &'static str {
    match s {
        SkillSource::User => "user",
        SkillSource::Project => "project",
        SkillSource::Plugin(_) => "plugin",
    }
}

pub fn render(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);
    render_list(f, cols[0], app, theme);
    render_detail(f, cols[1], app, theme);
}

fn render_list(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let visible = app.visible();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" skills · sort:{} ", app.sort.label()));
    if visible.is_empty() {
        let msg = if app.filter.is_empty() {
            "no skills found".to_string()
        } else {
            format!("no skills match '{}'", app.filter)
        };
        f.render_widget(Paragraph::new(msg).block(block), area);
        return;
    }
    let mut last_group: Option<&'static str> = None;
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_row = 0usize;
    for s in &visible {
        if app.group_by_source {
            let label = source_label(&s.source);
            if last_group != Some(label) {
                last_group = Some(label);
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("── {label} ──"),
                    Style::default().fg(theme.dim()),
                ))));
            }
        }
        if Some(s.name.as_str()) == app.selected.as_deref() {
            selected_row = items.len();
        }
        let name_w = 26usize;
        let name: String = if s.name.chars().count() > name_w {
            let cut: String = s.name.chars().take(name_w - 1).collect();
            format!("{cut}…")
        } else {
            format!("{:<name_w$}", s.name)
        };
        let row = if s.disabled {
            Line::from(vec![
                Span::styled("○ ", Style::default().fg(theme.dim())),
                Span::styled(name, Style::default().fg(theme.dim())),
                Span::styled(
                    format!("{:>4} ", s.usage.count),
                    Style::default().fg(theme.dim()),
                ),
                Span::styled(
                    "off",
                    Style::default()
                        .fg(theme.dim())
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    glyph(s.temperature),
                    Style::default().fg(theme.heat(s.temperature)),
                ),
                Span::raw(" "),
                Span::raw(name),
                Span::styled(
                    format!("{:>4} ", s.usage.count),
                    Style::default().fg(theme.dim()),
                ),
                Span::styled(
                    spark(&s.weekly),
                    Style::default().fg(theme.heat(s.temperature)),
                ),
            ])
        };
        items.push(ListItem::new(row));
    }
    let mut state = ListState::default();
    state.select(Some(selected_row));
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_detail(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(s) = app.selected_skill() else {
        f.render_widget(
            Paragraph::new("select a skill").block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    };
    let report = app.report.as_ref().expect("selected implies report");
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(5)])
        .split(area);

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                glyph(s.temperature),
                Style::default().fg(theme.heat(s.temperature)),
            ),
            Span::raw(" "),
            Span::styled(
                s.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
        Line::raw(
            s.description
                .clone()
                .unwrap_or_else(|| "(no description)".into()),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled("source  ", Style::default().fg(theme.dim())),
            Span::raw(source_label(&s.source)),
        ]),
        Line::from(vec![
            Span::styled("path    ", Style::default().fg(theme.dim())),
            Span::raw(s.path.display().to_string()),
        ]),
        Line::from(vec![
            Span::styled("tokens  ", Style::default().fg(theme.dim())),
            Span::raw(format!(
                "always-on {} · on-fire {}",
                format_tokens(s.cost.always_on),
                format_tokens(s.cost.on_fire)
            )),
        ]),
        Line::from(vec![
            Span::styled("used    ", Style::default().fg(theme.dim())),
            Span::raw(format!(
                "{} times · {}",
                s.usage.count,
                ago(s.usage.last_used, report.generated_at)
            )),
        ]),
        Line::raw(""),
    ];
    if let Some(h) = &s.history {
        lines.push(Line::from(vec![
            Span::styled("typed   ", Style::default().fg(theme.dim())),
            Span::raw(format!(
                "{}× · {}{}",
                h.count,
                ago(h.last_used, report.generated_at),
                if s.usage.count == 0 {
                    " — absent from transcripts"
                } else {
                    ""
                }
            )),
        ]));
    }
    if s.disabled {
        lines.push(Line::from(Span::styled(
            "disabled via enabledPlugins — not loaded",
            Style::default().fg(theme.dim()),
        )));
    }

    let outgoing: Vec<&str> = report
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::SkillMention && e.from == s.name)
        .map(|e| e.to.as_str())
        .collect();
    let incoming: Vec<&str> = report
        .edges
        .iter()
        .filter(|e| e.to == s.name)
        .map(|e| e.from.as_str())
        .collect();
    if !outgoing.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("→ mentions  ", Style::default().fg(theme.dim())),
            Span::raw(outgoing.join(", ")),
        ]));
    }
    if !incoming.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("← mentioned by  ", Style::default().fg(theme.dim())),
            Span::raw(incoming.join(", ")),
        ]));
    }
    for finding in report
        .findings
        .iter()
        .filter(|f| f.skill.as_deref() == Some(&s.name))
    {
        let (sym, color) = match finding.severity {
            Severity::Error => ("✗", theme.err()),
            Severity::Warn => ("!", theme.warn()),
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{sym} [{}] ", finding.code),
                Style::default().fg(color),
            ),
            Span::raw(finding.title.clone()),
        ]));
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" detail ")),
        rows[0],
    );

    let data: Vec<u64> = s.weekly.iter().map(|&v| v as u64).collect();
    f.render_widget(
        Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" 12-week trend "),
            )
            .style(Style::default().fg(theme.heat(s.temperature)))
            .data(&data),
        rows[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spark_all_zero_is_blank() {
        assert_eq!(spark(&[0; 12]), "            ");
    }

    #[test]
    fn spark_scales_to_max_and_keeps_positions() {
        let s = spark(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8]);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars.len(), 12);
        assert_eq!(chars[11], '█'); // max
        assert_eq!(chars[1], ' '); // zero stays blank
        assert!(chars[0] != ' ' && chars[0] != '█'); // small but visible
    }

    #[test]
    fn spark_uniform_nonzero_is_full_blocks() {
        assert_eq!(spark(&[3; 12]), "████████████");
    }
}
