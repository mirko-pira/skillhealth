use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const KEYMAP: &str = "\
 ↑↓ / jk      navigate list
 /            filter (esc clears)
 s            cycle sort: usage → name → temp → tokens
 S            group by source
 p            cycle scope: project → all → user
 L            toggle lens: project ↔ global
 enter / o    open in $EDITOR
 g            open HTML graph dashboard
 d / tab      toggle doctor
 y            copy selected fix (doctor)
 r            refresh now
 ?            this help
 q / esc      quit";

pub fn render(f: &mut Frame, area: Rect) {
    let [v] = Layout::vertical([Constraint::Length(17)])
        .flex(Flex::Center)
        .areas(area);
    let [target] = Layout::horizontal([Constraint::Length(56)])
        .flex(Flex::Center)
        .areas(v);
    f.render_widget(Clear, target);
    f.render_widget(
        Paragraph::new(KEYMAP).block(Block::default().borders(Borders::ALL).title(" keymap ")),
        target,
    );
}
