use ratatui::style::Color;

// Catppuccin Mocha
pub const BASE: Color = Color::Rgb(30, 30, 46); // #1e1e2e
pub const MANTLE: Color = Color::Rgb(24, 24, 37); // #181825
pub const TEXT: Color = Color::Rgb(205, 214, 244); // #cdd6f4
pub const SUBTEXT: Color = Color::Rgb(166, 173, 200);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
pub const YELLOW: Color = Color::Rgb(249, 226, 175);
pub const RED: Color = Color::Rgb(243, 139, 168);
pub const BLUE: Color = Color::Rgb(137, 180, 250);
pub const MAUVE: Color = Color::Rgb(203, 166, 247);
pub const TEAL: Color = Color::Rgb(148, 226, 213);
pub const PEACH: Color = Color::Rgb(250, 179, 135);
pub const OVERLAY: Color = Color::Rgb(69, 71, 90);

pub fn health_color(pct: f64) -> Color {
    if pct >= 85.0 {
        RED
    } else if pct >= 65.0 {
        YELLOW
    } else {
        GREEN
    }
}

pub fn budget_color(projected: f64, budget: f64) -> Color {
    if budget <= 0.0 {
        YELLOW
    } else if projected > budget {
        RED
    } else if projected > budget * 0.8 {
        YELLOW
    } else {
        GREEN
    }
}

// Border focus is handled inline via BLUE/OVERLAY in widgets.
