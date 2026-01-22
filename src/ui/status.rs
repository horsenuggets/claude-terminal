//! Status bar widget

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{styles, RenderState};

/// Draw the status bar
pub fn draw_status(frame: &mut Frame, area: Rect, state: &RenderState) {
    let mut spans = vec![];

    // Model
    spans.push(Span::styled(
        format!(" {} ", state.model),
        styles::model_style(),
    ));
    spans.push(Span::styled(" | ", styles::status_style()));

    // Status indicator
    if state.claude_busy {
        spans.push(Span::styled("Processing...", styles::busy_style()));
    } else {
        spans.push(Span::styled("Ready", styles::token_style()));
    }

    // Queue
    if state.message_queue_len > 0 {
        spans.push(Span::styled(" | ", styles::status_style()));
        spans.push(Span::styled(
            format!("{} queued", state.message_queue_len),
            styles::busy_style(),
        ));
    }

    // Status message
    if let Some(msg) = state.status_message {
        spans.push(Span::styled(" | ", styles::status_style()));
        spans.push(Span::styled(msg, styles::status_style()));
    }

    // Token usage (right aligned)
    let usage = state.token_usage;
    let token_info = format!(
        "In: {} Out: {} ",
        format_tokens(usage.input_tokens),
        format_tokens(usage.output_tokens)
    );

    // Calculate padding to right-align
    let left_len: usize = spans.iter().map(|s| s.content.len()).sum();
    let padding = (area.width as usize).saturating_sub(left_len + token_info.len());
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding)));
    }
    spans.push(Span::styled(token_info, styles::token_style()));

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
