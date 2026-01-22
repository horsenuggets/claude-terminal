//! Layout definitions

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
};

/// Create the main layout with conversation, input, and status areas
pub fn create_layout(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),        // Conversation (expandable)
            Constraint::Length(3),     // Input (fixed height)
            Constraint::Length(1),     // Status bar
        ])
        .split(area)
        .to_vec()
}
