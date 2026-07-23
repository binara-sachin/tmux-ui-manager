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
    /// Drag/drop accent (§8.1); consumed starting M2's move-mode highlighting.
    #[allow(dead_code)]
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
}
