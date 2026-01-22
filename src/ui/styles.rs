//! UI styles and colors (Catppuccin theme)

use ratatui::style::{Color, Modifier, Style};

// Catppuccin Mocha palette
pub const ROSEWATER: Color = Color::Rgb(245, 224, 220);
pub const FLAMINGO: Color = Color::Rgb(242, 205, 205);
pub const PINK: Color = Color::Rgb(245, 194, 231);
pub const MAUVE: Color = Color::Rgb(203, 166, 247);
pub const RED: Color = Color::Rgb(243, 139, 168);
pub const MAROON: Color = Color::Rgb(235, 160, 172);
pub const PEACH: Color = Color::Rgb(250, 179, 135);
pub const YELLOW: Color = Color::Rgb(249, 226, 175);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
pub const TEAL: Color = Color::Rgb(148, 226, 213);
pub const SKY: Color = Color::Rgb(137, 220, 235);
pub const SAPPHIRE: Color = Color::Rgb(116, 199, 236);
pub const BLUE: Color = Color::Rgb(137, 180, 250);
pub const LAVENDER: Color = Color::Rgb(180, 190, 254);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const SUBTEXT1: Color = Color::Rgb(186, 194, 222);
pub const SUBTEXT0: Color = Color::Rgb(166, 173, 200);
pub const OVERLAY2: Color = Color::Rgb(147, 153, 178);
pub const OVERLAY1: Color = Color::Rgb(127, 132, 156);
pub const OVERLAY0: Color = Color::Rgb(108, 112, 134);
pub const SURFACE2: Color = Color::Rgb(88, 91, 112);
pub const SURFACE1: Color = Color::Rgb(69, 71, 90);
pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
pub const BASE: Color = Color::Rgb(30, 30, 46);
pub const MANTLE: Color = Color::Rgb(24, 24, 37);
pub const CRUST: Color = Color::Rgb(17, 17, 27);

// Role-specific styles
pub fn user_style() -> Style {
    Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD)
}

pub fn assistant_style() -> Style {
    Style::default().fg(MAUVE)
}

pub fn system_style() -> Style {
    Style::default().fg(OVERLAY1).add_modifier(Modifier::ITALIC)
}

pub fn tool_style() -> Style {
    Style::default().fg(PEACH)
}

pub fn tool_result_style() -> Style {
    Style::default().fg(GREEN)
}

pub fn bash_style() -> Style {
    Style::default().fg(YELLOW)
}

pub fn thinking_style() -> Style {
    Style::default().fg(OVERLAY0).add_modifier(Modifier::ITALIC)
}

pub fn error_style() -> Style {
    Style::default().fg(RED)
}

pub fn recording_style() -> Style {
    Style::default().fg(RED).add_modifier(Modifier::BOLD)
}

pub fn busy_style() -> Style {
    Style::default().fg(YELLOW)
}

pub fn border_style() -> Style {
    Style::default().fg(SURFACE2)
}

pub fn input_style() -> Style {
    Style::default().fg(TEXT)
}

pub fn cursor_style() -> Style {
    Style::default().fg(BASE).bg(TEXT)
}

pub fn status_style() -> Style {
    Style::default().fg(SUBTEXT0)
}

pub fn model_style() -> Style {
    Style::default().fg(BLUE)
}

pub fn token_style() -> Style {
    Style::default().fg(GREEN)
}
