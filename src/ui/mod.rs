//! UI components using ratatui

mod conversation;
mod input;
mod layout;
mod status;
mod styles;

pub use conversation::*;
pub use input::*;
pub use layout::*;
pub use status::*;
pub use styles::*;

use ratatui::Frame;

use crate::app::{ConversationEntry, TokenUsage};

/// Input mode for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal text input
    Normal,
    /// Recording voice
    Recording,
}

/// State needed for rendering (borrowed references)
pub struct RenderState<'a> {
    pub messages: &'a [ConversationEntry],
    pub input: &'a str,
    pub cursor_position: usize,
    pub input_mode: InputMode,
    pub claude_busy: bool,
    pub streaming_buffer: &'a str,
    pub model: &'a str,
    pub scroll_offset: usize,
    pub status_message: Option<&'a str>,
    pub token_usage: &'a TokenUsage,
    pub message_queue_len: usize,
}

/// Main draw function
pub fn draw(frame: &mut Frame, state: &RenderState) {
    let chunks = create_layout(frame.area());

    // Draw conversation area
    draw_conversation(frame, chunks[0], state);

    // Draw input area
    draw_input(frame, chunks[1], state);

    // Draw status bar
    draw_status(frame, chunks[2], state);
}
