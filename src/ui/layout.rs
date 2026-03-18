//! Layout definitions

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
};

/// Create the main layout with conversation, input, and status areas
/// input_lines: number of lines in the input (1 for single line, more for multiline)
pub fn create_layout(area: Rect, input_lines: u16) -> Vec<Rect> {
    // Input height = top border (1) + content lines + bottom border (1)
    let input_height = input_lines + 2;

    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),              // Conversation (expandable)
            Constraint::Length(input_height), // Input (dynamic based on content)
            Constraint::Length(1),           // Status bar
        ])
        .split(area)
        .to_vec()
}
