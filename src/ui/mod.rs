//! UI components using ratatui

mod conversation;
mod input;
mod layout;
mod status;
mod styles;
mod wrap;

pub use conversation::*;
pub use input::*;
pub use layout::*;
pub use status::*;
#[allow(unused_imports)]
pub use styles::*;
pub use wrap::*;

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
    pub autocomplete_suggestion: Option<&'a str>,
    pub animation_tick: u8,
    pub total_messages: usize,
    pub verbose_mode: bool,
}

/// Calculate visual lines needed for input (accounting for word wrapping)
fn calculate_input_visual_lines(input: &str, width: u16) -> u16 {
    // Available width for text (subtract 2 for "> " or "  " prefix)
    let text_width = width.saturating_sub(2) as usize;
    if text_width == 0 {
        return 1;
    }

    calculate_wrapped_line_count(input, text_width) as u16
}

/// Main draw function
pub fn draw(frame: &mut Frame, state: &RenderState) {
    // Calculate visual lines needed (accounting for wrapping)
    let input_lines = calculate_input_visual_lines(state.input, frame.area().width);
    let chunks = create_layout(frame.area(), input_lines);

    // Draw conversation area
    draw_conversation(frame, chunks[0], state);

    // Draw input area
    draw_input(frame, chunks[1], state);

    // Draw status bar
    draw_status(frame, chunks[2], state);
}
