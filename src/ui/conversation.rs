//! Conversation view widget

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{ConversationContent, Role};

use super::{styles, RenderState};

/// Draw the conversation area
pub fn draw_conversation(frame: &mut Frame, area: Rect, state: &RenderState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(styles::border_style())
        .title(" Conversation ");

    let inner = block.inner(area);

    // Build lines from messages
    let mut lines: Vec<Line> = Vec::new();

    for entry in state.messages {
        let (prefix, style) = match entry.role {
            Role::User => ("You", styles::user_style()),
            Role::Assistant => ("Claude", styles::assistant_style()),
            Role::System => ("System", styles::system_style()),
            Role::Tool => ("Tool", styles::tool_style()),
            Role::Bash => ("Bash", styles::bash_style()),
        };

        match &entry.content {
            ConversationContent::Text(text) => {
                // Add role header
                lines.push(Line::from(vec![
                    Span::styled(format!("{}: ", prefix), style),
                ]));
                // Add content with word wrapping handled by Paragraph
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(styles::TEXT))));
                }
                lines.push(Line::from(""));
            }
            ConversationContent::ToolUse { name, input } => {
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", prefix), style),
                    Span::styled(name, styles::tool_style().add_modifier(Modifier::BOLD)),
                ]));
                // Truncate long inputs
                let display_input = if input.len() > 200 {
                    format!("{}...", &input[..200])
                } else {
                    input.clone()
                };
                lines.push(Line::from(Span::styled(
                    format!("  {}", display_input),
                    styles::tool_style(),
                )));
                lines.push(Line::from(""));
            }
            ConversationContent::ToolResult { name, result } => {
                lines.push(Line::from(vec![
                    Span::styled(format!("{} result: ", name), styles::tool_result_style()),
                ]));
                // Truncate long results
                let display_result = if result.len() > 500 {
                    format!("{}...", &result[..500])
                } else {
                    result.clone()
                };
                for line in display_result.lines().take(10) {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        styles::tool_result_style(),
                    )));
                }
                lines.push(Line::from(""));
            }
            ConversationContent::Thinking(text) => {
                lines.push(Line::from(vec![
                    Span::styled("Thinking: ", styles::thinking_style()),
                ]));
                // Show truncated thinking
                let display = if text.len() > 300 {
                    format!("{}...", &text[..300])
                } else {
                    text.clone()
                };
                lines.push(Line::from(Span::styled(display, styles::thinking_style())));
                lines.push(Line::from(""));
            }
            ConversationContent::BashCommand {
                command,
                output,
                exit_code,
            } => {
                lines.push(Line::from(vec![
                    Span::styled("$ ", styles::bash_style()),
                    Span::styled(command, styles::bash_style().add_modifier(Modifier::BOLD)),
                ]));
                // Show output
                for line in output.lines().take(20) {
                    lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(styles::TEXT))));
                }
                if output.lines().count() > 20 {
                    lines.push(Line::from(Span::styled(
                        "  ... (output truncated)",
                        styles::system_style(),
                    )));
                }
                // Show exit code if non-zero
                if *exit_code != 0 {
                    lines.push(Line::from(Span::styled(
                        format!("(exit code: {})", exit_code),
                        styles::error_style(),
                    )));
                }
                lines.push(Line::from(""));
            }
        }
    }

    // Add streaming buffer if present
    if !state.streaming_buffer.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Claude: ", styles::assistant_style()),
        ]));
        for line in state.streaming_buffer.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(styles::TEXT))));
        }
        // Show typing indicator
        lines.push(Line::from(Span::styled("...", styles::busy_style())));
    }

    // Calculate scroll
    let visible_height = inner.height as usize;
    let total_lines = lines.len();
    let scroll = if total_lines > visible_height {
        let max_scroll = total_lines.saturating_sub(visible_height);
        max_scroll.saturating_sub(state.scroll_offset)
    } else {
        0
    };

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);
}
