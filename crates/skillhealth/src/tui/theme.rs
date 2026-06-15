use ratatui::style::Color;
use ratatui::text::Span;
use skillhealth_core::model::Temperature;

// Heat palette — MUST stay identical to the static render + HTML dashboard.
pub const HOT: Color = Color::Rgb(0xef, 0x44, 0x44);
pub const WARM: Color = Color::Rgb(0xfb, 0xbf, 0x24);
pub const COLD: Color = Color::Rgb(0x60, 0xa5, 0xfa);
pub const DEAD: Color = Color::Rgb(0xe2, 0xe8, 0xf0);
// Doctor semantics — decoupled from heat, never reuse state colors.
pub const OK: Color = Color::Rgb(0x34, 0xd3, 0x99);
pub const ERR: Color = Color::Rgb(0xf8, 0x71, 0x71);

pub fn glyph(t: Temperature) -> &'static str {
    match t {
        Temperature::Hot => "●",
        Temperature::Warm => "◐",
        Temperature::Cold => "○",
        Temperature::Dead => "◌",
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    /// false under NO_COLOR: monochrome glyphs, fully interactive.
    pub color: bool,
}

impl Theme {
    pub fn from_env() -> Self {
        Theme {
            color: std::env::var_os("NO_COLOR").is_none(),
        }
    }

    fn paint(&self, c: Color) -> Color {
        if self.color { c } else { Color::Reset }
    }

    pub fn heat(&self, t: Temperature) -> Color {
        self.paint(match t {
            Temperature::Hot => HOT,
            Temperature::Warm => WARM,
            Temperature::Cold => COLD,
            Temperature::Dead => DEAD,
        })
    }

    pub fn ok(&self) -> Color {
        self.paint(OK)
    }
    pub fn err(&self) -> Color {
        self.paint(ERR)
    }
    pub fn warn(&self) -> Color {
        self.paint(WARM)
    }
    pub fn dim(&self) -> Color {
        self.paint(Color::Rgb(0x64, 0x74, 0x8b))
    }
}

/// 24-bit gradient across the chars of `text`, HOT → COLD. Mono: plain spans.
pub fn gradient_spans(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len().max(2) - 1;
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if !theme.color {
                return Span::raw(c.to_string());
            }
            let f = i as f32 / n as f32;
            let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f) as u8;
            Span::styled(
                c.to_string(),
                ratatui::style::Style::default().fg(Color::Rgb(
                    lerp(0xef, 0x60),
                    lerp(0x44, 0xa5),
                    lerp(0x44, 0xfa),
                )),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillhealth_core::model::Temperature;

    #[test]
    fn glyphs_match_spec() {
        assert_eq!(glyph(Temperature::Hot), "●");
        assert_eq!(glyph(Temperature::Warm), "◐");
        assert_eq!(glyph(Temperature::Cold), "○");
        assert_eq!(glyph(Temperature::Dead), "◌");
    }

    #[test]
    fn heat_palette_matches_dashboard_hex() {
        let t = Theme { color: true };
        assert_eq!(t.heat(Temperature::Hot), Color::Rgb(0xef, 0x44, 0x44));
        assert_eq!(t.heat(Temperature::Warm), Color::Rgb(0xfb, 0xbf, 0x24));
        assert_eq!(t.heat(Temperature::Cold), Color::Rgb(0x60, 0xa5, 0xfa));
        assert_eq!(t.heat(Temperature::Dead), Color::Rgb(0xe2, 0xe8, 0xf0));
    }

    #[test]
    fn no_color_degrades_to_reset() {
        let t = Theme { color: false };
        assert_eq!(t.heat(Temperature::Hot), Color::Reset);
        assert_eq!(t.ok(), Color::Reset);
        assert_eq!(t.err(), Color::Reset);
    }

    #[test]
    fn gradient_spans_one_per_char_and_mono_has_no_fg() {
        let spans = gradient_spans("skillhealth", &Theme { color: true });
        assert_eq!(spans.len(), "skillhealth".chars().count());
        assert!(
            spans
                .iter()
                .all(|s| matches!(s.style.fg, Some(Color::Rgb(..))))
        );
        let mono = gradient_spans("skillhealth", &Theme { color: false });
        assert!(mono.iter().all(|s| s.style.fg.is_none()));
    }
}
