//! Input field widget

use ratatui::{
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::{styles, word_wrap, InputMode, RenderState};

/// Draw the input area (horizontal line separator at top and bottom)
pub fn draw_input(frame: &mut Frame, area: Rect, state: &RenderState) {
    let border_style = match state.input_mode {
        InputMode::Normal => styles::border_style(),
        InputMode::Recording => styles::recording_style(),
    };

    // Top and bottom borders (horizontal line separators)
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(border_style);

    let input = state.input;
    let cursor_pos = state.cursor_position;

    // Build input with prompt
    let prompt_style = if state.input_mode == InputMode::Recording {
        styles::recording_style()
    } else {
        styles::user_style()
    };

    // Calculate inner area before consuming block
    let inner = block.inner(area);
    let text_width = inner.width.saturating_sub(2) as usize; // Width after "> " or "  "

    // Build visual lines with word wrapping
    let mut text_lines: Vec<Line> = Vec::new();
    let logical_lines: Vec<&str> = input.split('\n').collect();

    for (i, line_text) in logical_lines.iter().enumerate() {
        let is_first_logical = i == 0;

        if line_text.is_empty() {
            // Empty line
            let prefix = if is_first_logical { "> " } else { "  " };
            text_lines.push(Line::from(Span::styled(prefix, prompt_style)));
        } else {
            // Word-wrap this logical line
            let wrapped = word_wrap(line_text, text_width);
            for (j, wrapped_line) in wrapped.iter().enumerate() {
                let is_first_visual = j == 0;
                let is_last_visual = j == wrapped.len() - 1;

                let mut spans = vec![];
                if is_first_logical && is_first_visual {
                    spans.push(Span::styled("> ", prompt_style));
                } else {
                    spans.push(Span::styled("  ", prompt_style));
                }
                spans.push(Span::styled(wrapped_line.clone(), styles::input_style()));

                // Add autocomplete hint only to the last visual line of first logical line
                if is_first_logical && is_last_visual {
                    if let Some(suggestion) = state.autocomplete_suggestion {
                        if suggestion.len() > input.len() {
                            let hint = &suggestion[input.len()..];
                            spans.push(Span::styled(hint, styles::thinking_style()));
                            spans.push(Span::styled(" (Tab)", styles::thinking_style()));
                        }
                    }
                }

                text_lines.push(Line::from(spans));
            }
        }
    }

    let paragraph = Paragraph::new(Text::from(text_lines)).block(block);

    frame.render_widget(paragraph, area);

    // Calculate cursor position accounting for newlines and word wrapping
    let text_width = inner.width.saturating_sub(2) as usize; // Available width after prompt
    let mut cursor_line: u16 = 0;
    let mut cursor_col: usize = 2; // Start after "> "

    if text_width > 0 {
        // Find which logical line the cursor is on and the position within that line
        let mut chars_before_cursor = cursor_pos;
        let logical_lines: Vec<&str> = input.split('\n').collect();

        for logical_line in logical_lines {
            let line_char_count = logical_line.chars().count();

            if chars_before_cursor <= line_char_count {
                // Cursor is in this logical line
                // Calculate cursor position character by character
                let chars: Vec<char> = logical_line.chars().collect();
                let mut visual_col = 0;
                let mut visual_line_offset: u16 = 0;

                for i in 0..chars_before_cursor {
                    let ch = chars.get(i).copied().unwrap_or(' ');
                    if ch == ' ' && visual_col >= text_width {
                        // Word wrap happens here, move to next line
                        visual_line_offset += 1;
                        visual_col = 0;
                    } else if visual_col >= text_width {
                        // Hard wrap within word
                        visual_line_offset += 1;
                        visual_col = 1;
                    } else {
                        visual_col += 1;
                    }
                }

                cursor_line += visual_line_offset;
                cursor_col = 2 + visual_col;
                break;
            }

            // Move past this logical line (including newline)
            chars_before_cursor -= line_char_count + 1;
            // Add visual lines for this logical line
            let wrapped = word_wrap(logical_line, text_width);
            cursor_line += wrapped.len() as u16;
        }
    }

    let cursor_x = inner.x + cursor_col as u16;
    let cursor_y = inner.y + cursor_line;

    if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
