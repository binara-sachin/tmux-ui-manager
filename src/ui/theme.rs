use ratatui::style::Color;

/// Catppuccin Mocha palette (§8.1) plus the semantic mapping (§8.2) used throughout
/// the UI. Each field is overridable at startup via a `@manager-color-*` tmux option;
/// `Theme::default()` holds the hardcoded Mocha values.
// §8.1 hardcodes the full Mocha palette even though v1's semantic mapping only
// assigns a role to a subset of it (crust/yellow/mauve/lavender have none yet).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub base: Color,
    pub mantle: Color,
    pub crust: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub overlay0: Color,
    pub text: Color,
    pub subtext0: Color,
    pub green: Color,
    pub blue: Color,
    pub red: Color,
    pub yellow: Color,
    pub mauve: Color,
    pub lavender: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            base: hex(0x1e1e2e),
            mantle: hex(0x181825),
            crust: hex(0x11111b),
            surface0: hex(0x313244),
            surface1: hex(0x45475a),
            overlay0: hex(0x6c7086),
            text: hex(0xcdd6f4),
            subtext0: hex(0xa6adc8),
            green: hex(0xa6e3a1),
            blue: hex(0x89b4fa),
            red: hex(0xf38ba8),
            yellow: hex(0xf9e2af),
            mauve: hex(0xcba6f7),
            lavender: hex(0xb4befe),
        }
    }
}

impl Theme {
    // Semantic accessors (§8.1 "Semantic mapping").
    pub fn bg(&self) -> Color {
        self.base
    }
    pub fn panel_title(&self) -> Color {
        self.overlay0
    }
    pub fn fg(&self) -> Color {
        self.text
    }
    pub fn meta(&self) -> Color {
        self.subtext0
    }
    pub fn active(&self) -> Color {
        self.green
    }
    /// Drag/drop accent (§8.1).
    pub fn accent(&self) -> Color {
        self.blue
    }
    /// Destructive/error color (§8.1); consumed starting M1's kill confirm/toasts.
    #[allow(dead_code)]
    pub fn danger(&self) -> Color {
        self.red
    }
    pub fn border(&self) -> Color {
        self.surface1
    }
    pub fn selection_bg(&self) -> Color {
        self.surface0
    }
}

const fn hex(rgb: u32) -> Color {
    Color::Rgb(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    )
}

/// Parses a literal `#rrggbb` hex color (§8.2: "literal hex only... no
/// references, no names"). `None` on anything else, so a malformed option
/// value is silently ignored rather than crashing startup.
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

impl Theme {
    /// Applies one `@manager-color-*` override (§8.2/§9) if `option` names a
    /// known semantic slot and `value` parses as a hex color; otherwise a
    /// no-op (unknown option names are ignored, not errors — tmux options
    /// this plugin doesn't know about are none of its business).
    pub fn apply_option(&mut self, option: &str, value: &str) {
        let Some(color) = parse_hex_color(value) else {
            return;
        };
        match option {
            "@manager-color-bg" => self.base = color,
            "@manager-color-text" => self.text = color,
            "@manager-color-meta" => self.subtext0 = color,
            "@manager-color-active" => self.green = color,
            "@manager-color-accent" => self.blue = color,
            "@manager-color-danger" => self.red = color,
            "@manager-color-border" => self.surface1 = color,
            "@manager-color-panel-title" => self.overlay0 = color,
            "@manager-color-selection-bg" => self.surface0 = color,
            _ => {}
        }
    }
}

/// Every `@manager-color-*` option name `Theme::apply_option` understands
/// (§8.2's semantic mapping — one option per named slot in §8.1's table).
pub const COLOR_OPTIONS: &[&str] = &[
    "@manager-color-bg",
    "@manager-color-text",
    "@manager-color-meta",
    "@manager-color-active",
    "@manager-color-accent",
    "@manager-color-danger",
    "@manager-color-border",
    "@manager-color-panel-title",
    "@manager-color-selection-bg",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_matches_mocha_spec() {
        assert_eq!(Theme::default().base, Color::Rgb(0x1e, 0x1e, 0x2e));
    }

    #[test]
    fn accent_is_blue() {
        assert_eq!(Theme::default().accent(), Color::Rgb(0x89, 0xb4, 0xfa));
    }

    #[test]
    fn parse_hex_color_accepts_lowercase_and_uppercase_rrggbb() {
        assert_eq!(
            parse_hex_color("#89b4fa"),
            Some(Color::Rgb(0x89, 0xb4, 0xfa))
        );
        assert_eq!(
            parse_hex_color("#89B4FA"),
            Some(Color::Rgb(0x89, 0xb4, 0xfa))
        );
    }

    #[test]
    fn parse_hex_color_rejects_anything_else() {
        assert_eq!(parse_hex_color("89b4fa"), None); // missing '#'
        assert_eq!(parse_hex_color("#89b4f"), None); // too short
        assert_eq!(parse_hex_color("#89b4faa"), None); // too long
        assert_eq!(parse_hex_color("#zzzzzz"), None); // not hex digits
        assert_eq!(parse_hex_color("blue"), None); // §8.2: names aren't allowed
    }

    #[test]
    fn apply_option_overrides_only_the_named_slot() {
        let mut theme = Theme::default();
        theme.apply_option("@manager-color-accent", "#ff0000");
        assert_eq!(theme.accent(), Color::Rgb(0xff, 0, 0));
        // Unrelated slots are untouched.
        assert_eq!(theme.bg(), Color::Rgb(0x1e, 0x1e, 0x2e));
    }

    #[test]
    fn apply_option_ignores_unknown_options_and_bad_values() {
        let mut theme = Theme::default();
        let before = theme.bg();
        theme.apply_option("@manager-color-nonsense", "#ff0000");
        theme.apply_option("@manager-color-bg", "not-a-color");
        assert_eq!(theme.bg(), before);
    }
}
