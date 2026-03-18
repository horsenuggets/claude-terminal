//! Status bar widget

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{styles, RenderState};

/// Calculate display width of a string (counts chars, not bytes)
fn display_width(s: &str) -> usize {
    s.chars().count()
}

/// Get animated dots based on tick (shared animation logic)
/// Uses modulo 64 for clean 4-state cycle (16 ticks per state)
/// 64 divides evenly into 256 (u8 max), preventing glitches on wrap
pub fn animated_dots(tick: u8) -> &'static str {
    match (tick % 64) / 16 {
        0 => "   ",
        1 => ".  ",
        2 => ".. ",
        _ => "...",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animated_dots_cycles_correctly() {
        // Test each state
        assert_eq!(animated_dots(0), "   ");
        assert_eq!(animated_dots(15), "   ");
        assert_eq!(animated_dots(16), ".  ");
        assert_eq!(animated_dots(31), ".  ");
        assert_eq!(animated_dots(32), ".. ");
        assert_eq!(animated_dots(47), ".. ");
        assert_eq!(animated_dots(48), "...");
        assert_eq!(animated_dots(63), "...");
        // Should wrap cleanly
        assert_eq!(animated_dots(64), "   ");
    }

    #[test]
    fn test_animated_dots_no_glitch_on_u8_wrap() {
        // Verify smooth transition across u8 boundary (255 -> 0)
        // 255 % 64 = 63, 63/16 = 3 -> "..."
        // 0 % 64 = 0, 0/16 = 0 -> "   "
        // This is a valid state transition (end of cycle -> start of cycle)
        assert_eq!(animated_dots(255), "...");
        assert_eq!(animated_dots(0), "   ");

        // Verify 256 wraps correctly (256 % 64 = 0)
        // 192 + 64 = 256 which wraps to 0
        assert_eq!(animated_dots(192), "   "); // 192 % 64 = 0
        assert_eq!(animated_dots(208), ".  "); // 208 % 64 = 16
        assert_eq!(animated_dots(224), ".. "); // 224 % 64 = 32
        assert_eq!(animated_dots(240), "..."); // 240 % 64 = 48
    }

    #[test]
    fn test_animated_dots_full_cycle() {
        // Verify a full 64-tick cycle shows all states in order
        let mut states = Vec::new();
        let mut last_state = "";
        for tick in 0..64 {
            let state = animated_dots(tick);
            if state != last_state {
                states.push(state);
                last_state = state;
            }
        }
        assert_eq!(states, vec!["   ", ".  ", ".. ", "..."]);
    }
}

/// Draw the status bar
pub fn draw_status(frame: &mut Frame, area: Rect, state: &RenderState) {
    let mut spans = vec![];

    // Left padding + Model
    spans.push(Span::styled(
        format!(" {} ", state.model),
        styles::model_style(),
    ));
    spans.push(Span::styled("| ", styles::dim_style()));

    // Status indicator with animated dots
    if state.claude_busy {
        let dots = animated_dots(state.animation_tick);
        spans.push(Span::styled(format!("Processing{}", dots), styles::busy_style()));
    } else {
        spans.push(Span::styled("Ready", styles::token_style()));
    }

    // Queue count (only if no status message, to avoid duplication)
    if state.message_queue_len > 0 && state.status_message.is_none() {
        spans.push(Span::styled(" | ", styles::dim_style()));
        spans.push(Span::styled(
            format!("{} queued", state.message_queue_len),
            styles::busy_style(),
        ));
    }

    // Status message (takes precedence over queue display)
    if let Some(msg) = state.status_message {
        spans.push(Span::styled(" | ", styles::dim_style()));
        // Animate dots for "Transcribing" message
        if msg == "Transcribing" {
            let dots = animated_dots(state.animation_tick);
            spans.push(Span::styled(format!("{}{}", msg, dots), styles::busy_style()));
        } else {
            spans.push(Span::styled(msg, styles::busy_style()));
        }
    }

    // Debug: show scroll offset when scrolled up
    if state.scroll_offset > 0 {
        spans.push(Span::styled(" | ", styles::dim_style()));
        spans.push(Span::styled(
            format!("scroll:{}", state.scroll_offset),
            styles::dim_style(),
        ));
    }

    // Token usage (right aligned)
    let usage = state.token_usage;
    let token_info = format!(
        "in:{} out:{} ",
        format_tokens(usage.input_tokens),
        format_tokens(usage.output_tokens)
    );

    // Calculate padding using character count
    let left_width: usize = spans.iter().map(|s| display_width(&s.content)).sum();
    let token_width = display_width(&token_info);
    let padding = (area.width as usize).saturating_sub(left_width + token_width);
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding)));
    }
    spans.push(Span::styled(token_info, styles::dim_style()));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);

    frame.render_widget(paragraph, area);
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}
