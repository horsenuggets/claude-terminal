//! Input field widget

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::{styles, InputMode, RenderState};

/// Draw the input area
pub fn draw_input(frame: &mut Frame, area: Rect, state: &RenderState) {
    let (title, border_style) = match state.input_mode {
        InputMode::Normal => (" Input ", styles::border_style()),
        InputMode::Recording => (" Recording... (press * to stop) ", styles::recording_style()),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    // Build input line with vertical bar cursor
    let input = state.input;
    let cursor_pos = state.cursor_position;

    let (before_cursor, after_cursor) = if cursor_pos <= input.len() {
        let (before, after) = input.split_at(cursor_pos);
        (before.to_string(), after.to_string())
    } else {
        (input.to_string(), String::new())
    };

    let line = Line::from(vec![
        Span::styled("  ", styles::input_style()), // Left padding
        Span::styled(before_cursor, styles::input_style()),
        Span::styled("│", styles::cursor_style()),
        Span::styled(after_cursor, styles::input_style()),
    ]);

    let paragraph = Paragraph::new(line).block(block);

    frame.render_widget(paragraph, area);

    // Set cursor position (accounting for border + padding)
    let x = area.x + 1 + 2 + cursor_pos as u16; // +1 border, +2 padding
    let y = area.y + 1;
    if x < area.x + area.width - 1 {
        frame.set_cursor_position((x, y));
    }
}
