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

    // Build input line with cursor
    let input = state.input;
    let cursor_pos = state.cursor_position;

    let (before_cursor, cursor_char, after_cursor) = if cursor_pos < input.len() {
        let (before, rest) = input.split_at(cursor_pos);
        let mut chars = rest.chars();
        let cursor = chars.next().unwrap_or(' ');
        (before.to_string(), cursor, chars.collect::<String>())
    } else {
        (input.to_string(), ' ', String::new())
    };

    let line = Line::from(vec![
        Span::styled(before_cursor, styles::input_style()),
        Span::styled(cursor_char.to_string(), styles::cursor_style()),
        Span::styled(after_cursor, styles::input_style()),
    ]);

    let paragraph = Paragraph::new(line).block(block);

    frame.render_widget(paragraph, area);

    // Set cursor position
    let x = area.x + 1 + cursor_pos as u16;
    let y = area.y + 1;
    if x < area.x + area.width - 1 {
        frame.set_cursor_position((x, y));
    }
}
