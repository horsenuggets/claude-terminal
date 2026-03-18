//! Conversation view widget (Claude Code style)

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};
use serde_json;

use crate::app::{ConversationContent, Role};

use super::{styles, word_wrap, RenderState};

/// Draw the conversation area (Claude Code style - no borders, dots for indicators)
pub fn draw_conversation(frame: &mut Frame, area: Rect, state: &RenderState) {
    let inner = area;
    let width = area.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    // Group consecutive tool/bash messages for collapsed view
    let mut i = 0;
    while i < state.messages.len() {
        let entry = &state.messages[i];
        match entry.role {
            Role::User => {
                render_user_message(&mut lines, &entry.content, width);
                i += 1;
            }
            Role::Assistant => {
                render_assistant_message(&mut lines, &entry.content);
                i += 1;
            }
            Role::System => {
                render_system_message(&mut lines, &entry.content);
                i += 1;
            }
            Role::Tool | Role::Bash => {
                if state.verbose_mode {
                    // Verbose: show each tool individually
                    if entry.role == Role::Tool {
                        render_tool_message(&mut lines, &entry.content, true);
                    } else {
                        render_bash_message(&mut lines, &entry.content, true);
                    }
                    i += 1;
                } else {
                    // Collapsed: count consecutive tool/bash messages
                    let mut tool_count = 0;
                    let mut has_edit = false;
                    let mut edit_content = None;

                    while i < state.messages.len() {
                        let msg = &state.messages[i];
                        match msg.role {
                            Role::Tool => {
                                tool_count += 1;
                                // Check if it's an Edit (we always show edit diffs)
                                if let ConversationContent::ToolUse { name, .. } = &msg.content {
                                    if name == "Edit" {
                                        has_edit = true;
                                        edit_content = Some(&msg.content);
                                    }
                                }
                                i += 1;
                            }
                            Role::Bash => {
                                tool_count += 1;
                                i += 1;
                            }
                            _ => break,
                        }
                    }

                    // Render collapsed summary
                    if tool_count > 0 {
                        let summary = if tool_count == 1 {
                            "1 tool".to_string()
                        } else {
                            format!("{} tools", tool_count)
                        };
                        lines.push(Line::from(vec![
                            Span::styled(styles::DOT, styles::dot_tool()),
                            Span::styled(summary, styles::dim_style()),
                        ]));

                        // Always show edit diffs even in collapsed mode
                        if has_edit {
                            if let Some(content) = edit_content {
                                render_tool_message(&mut lines, content, true);
                            }
                        }
                    }
                }
            }
        }
    }

    // Add streaming buffer if present (Claude is responding)
    if !state.streaming_buffer.is_empty() {
        // Show white dot with streaming text
        let first_line = state.streaming_buffer.lines().next().unwrap_or("");
        lines.push(Line::from(vec![
            Span::styled(styles::DOT, styles::dot_normal()),
            Span::styled(first_line.to_string(), Style::default().fg(styles::TEXT)),
        ]));
        // Additional lines without dot
        for line in state.streaming_buffer.lines().skip(1) {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(styles::TEXT),
            )));
        }
    }

    // Calculate scroll - auto-scroll to bottom unless user scrolled up
    let visible_height = inner.height as usize;
    let total_lines = lines.len();
    let scroll = if total_lines > visible_height {
        let max_scroll = total_lines.saturating_sub(visible_height);
        // Clamp scroll_offset to valid range, then calculate scroll position
        let clamped_offset = state.scroll_offset.min(max_scroll);
        max_scroll.saturating_sub(clamped_offset)
    } else {
        0
    };

    let paragraph = Paragraph::new(Text::from(lines))
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);
}

fn render_user_message(lines: &mut Vec<Line>, content: &ConversationContent, width: usize) {
    if let ConversationContent::Text(text) = content {
        // User messages with gray background, word-wrapped to full width
        // Leave 1 char padding on each side
        let text_width = width.saturating_sub(2);
        if text_width == 0 {
            return;
        }

        for paragraph in text.lines() {
            if paragraph.is_empty() {
                // Empty line - just show full-width background
                let padded = " ".repeat(width);
                lines.push(Line::from(Span::styled(padded, styles::user_message_style())));
                continue;
            }

            // Word-wrap the paragraph
            let wrapped = word_wrap(paragraph, text_width);
            for wrapped_line in wrapped {
                // Pad to full width: " text... " with spaces to fill
                let content_len = wrapped_line.chars().count();
                let right_padding = width.saturating_sub(content_len + 1); // +1 for left space
                let padded = format!(" {}{}", wrapped_line, " ".repeat(right_padding));
                lines.push(Line::from(Span::styled(padded, styles::user_message_style())));
            }
        }
        lines.push(Line::from(""));
    }
}


/// Parse a line of markdown and return styled spans
/// Handles: headers (#), bold (**), italic (*), inline code (`)
fn parse_markdown_line(text: &str) -> Vec<Span<'static>> {
    // Check for headers at start of line
    let trimmed = text.trim_start();
    if trimmed.starts_with("### ") {
        let content = trimmed.strip_prefix("### ").unwrap_or(trimmed);
        return vec![Span::styled(content.to_string(), styles::header_style())];
    } else if trimmed.starts_with("## ") {
        let content = trimmed.strip_prefix("## ").unwrap_or(trimmed);
        return vec![Span::styled(content.to_string(), styles::header_style())];
    } else if trimmed.starts_with("# ") {
        let content = trimmed.strip_prefix("# ").unwrap_or(trimmed);
        return vec![Span::styled(content.to_string(), styles::header_style())];
    }

    // Parse inline formatting: **bold**, *italic*, `code`
    parse_inline_formatting(text)
}

/// Parse inline formatting: **bold**, *italic*, `code`
fn parse_inline_formatting(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current = String::new();
    let normal_style = Style::default().fg(styles::TEXT);

    while let Some(ch) = chars.next() {
        match ch {
            '`' => {
                // Push accumulated text
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), normal_style));
                    current.clear();
                }
                // Collect until closing backtick
                let mut code = String::new();
                let mut found_close = false;
                for inner_ch in chars.by_ref() {
                    if inner_ch == '`' {
                        found_close = true;
                        break;
                    }
                    code.push(inner_ch);
                }
                if found_close && !code.is_empty() {
                    spans.push(Span::styled(code, styles::inline_code_style()));
                } else {
                    current.push('`');
                    current.push_str(&code);
                }
            }
            '*' => {
                // Check for ** (bold) or * (italic)
                if chars.peek() == Some(&'*') {
                    chars.next(); // consume second *
                    // Push accumulated text
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), normal_style));
                        current.clear();
                    }
                    // Collect until **
                    let mut bold_text = String::new();
                    let mut found_close = false;
                    while let Some(inner_ch) = chars.next() {
                        if inner_ch == '*' && chars.peek() == Some(&'*') {
                            chars.next(); // consume second *
                            found_close = true;
                            break;
                        }
                        bold_text.push(inner_ch);
                    }
                    if found_close && !bold_text.is_empty() {
                        spans.push(Span::styled(bold_text, styles::bold_style()));
                    } else {
                        current.push_str("**");
                        current.push_str(&bold_text);
                    }
                } else {
                    // Single * for italic
                    // Push accumulated text
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), normal_style));
                        current.clear();
                    }
                    // Collect until *
                    let mut italic_text = String::new();
                    let mut found_close = false;
                    for inner_ch in chars.by_ref() {
                        if inner_ch == '*' {
                            found_close = true;
                            break;
                        }
                        italic_text.push(inner_ch);
                    }
                    if found_close && !italic_text.is_empty() {
                        spans.push(Span::styled(italic_text, styles::italic_style()));
                    } else {
                        current.push('*');
                        current.push_str(&italic_text);
                    }
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    // Push remaining text
    if !current.is_empty() {
        spans.push(Span::styled(current, normal_style));
    }

    if spans.is_empty() {
        spans.push(Span::styled("".to_string(), normal_style));
    }

    spans
}

fn render_assistant_message(lines: &mut Vec<Line>, content: &ConversationContent) {
    match content {
        ConversationContent::Text(text) => {
            let text_lines: Vec<&str> = text.lines().collect();
            let mut in_code_block = false;
            let mut is_first_line = true;

            for line in text_lines.iter() {
                // Check for code block fence
                if line.trim().starts_with("```") {
                    in_code_block = !in_code_block;
                    // Don't render the fence line itself
                    if is_first_line {
                        lines.push(Line::from(vec![
                            Span::styled(styles::DOT, styles::dot_normal()),
                        ]));
                        is_first_line = false;
                    }
                    continue;
                }

                if in_code_block {
                    // Code block content - use code style with background
                    let prefix = if is_first_line {
                        is_first_line = false;
                        Span::styled(styles::DOT, styles::dot_normal())
                    } else {
                        Span::styled("  ", Style::default())
                    };
                    lines.push(Line::from(vec![
                        prefix,
                        Span::styled(format!("  {}", line), styles::code_block_style()),
                    ]));
                } else {
                    // Regular text with markdown parsing
                    let prefix = if is_first_line {
                        is_first_line = false;
                        Span::styled(styles::DOT, styles::dot_normal())
                    } else {
                        Span::styled("  ", Style::default().fg(styles::TEXT))
                    };
                    let mut spans = vec![prefix];
                    spans.extend(parse_markdown_line(line));
                    lines.push(Line::from(spans));
                }
            }
            lines.push(Line::from(""));
        }
        ConversationContent::Thinking(text) => {
            // Thinking in italics, dimmed
            let display = if text.len() > 200 {
                format!("{}...", &text[..200])
            } else {
                text.clone()
            };
            lines.push(Line::from(vec![
                Span::styled("  ", styles::dim_style()),
                Span::styled(display, styles::thinking_style()),
            ]));
        }
        _ => {}
    }
}

fn render_system_message(lines: &mut Vec<Line>, content: &ConversationContent) {
    match content {
        ConversationContent::Text(text) => {
            for line in text.lines() {
                lines.push(Line::from(Span::styled(line.to_string(), styles::system_style())));
            }
            lines.push(Line::from(""));
        }
        ConversationContent::Error(msg) => {
            // Error with red dot
            lines.push(Line::from(vec![
                Span::styled(styles::DOT, styles::dot_error()),
                Span::styled(msg.clone(), styles::error_style()),
            ]));
            lines.push(Line::from(""));
        }
        _ => {}
    }
}

fn render_tool_message(lines: &mut Vec<Line>, content: &ConversationContent, _show_details: bool) {
    match content {
        ConversationContent::ToolUse { name, input } => {
            // Parse input JSON to extract key info
            let parsed: Option<serde_json::Value> = serde_json::from_str(input).ok();

            // Format based on tool type
            let summary = match name.as_str() {
                "Read" => {
                    let path = parsed.as_ref()
                        .and_then(|v| v.get("file_path"))
                        .and_then(|v| v.as_str())
                        .map(shorten_path)
                        .unwrap_or_default();
                    format!("Read {}", path)
                }
                "Edit" => {
                    let path = parsed.as_ref()
                        .and_then(|v| v.get("file_path"))
                        .and_then(|v| v.as_str())
                        .map(shorten_path)
                        .unwrap_or_default();
                    format!("Edit {}", path)
                }
                "Write" => {
                    let path = parsed.as_ref()
                        .and_then(|v| v.get("file_path"))
                        .and_then(|v| v.as_str())
                        .map(shorten_path)
                        .unwrap_or_default();
                    format!("Write {}", path)
                }
                "Glob" => {
                    let pattern = parsed.as_ref()
                        .and_then(|v| v.get("pattern"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    format!("Glob {}", pattern)
                }
                "Grep" => {
                    let pattern = parsed.as_ref()
                        .and_then(|v| v.get("pattern"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let truncated = if pattern.len() > 30 {
                        format!("{}...", &pattern[..30])
                    } else {
                        pattern.to_string()
                    };
                    format!("Grep \"{}\"", truncated)
                }
                "Bash" => {
                    let cmd = parsed.as_ref()
                        .and_then(|v| v.get("command"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let truncated = if cmd.len() > 50 {
                        format!("{}...", &cmd[..50])
                    } else {
                        cmd.to_string()
                    };
                    format!("$ {}", truncated)
                }
                "Task" => {
                    let desc = parsed.as_ref()
                        .and_then(|v| v.get("description"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("agent task");
                    format!("Task({})", desc)
                }
                _ => name.clone(),
            };

            // Orange dot with compact summary
            lines.push(Line::from(vec![
                Span::styled(styles::DOT, styles::dot_tool()),
                Span::styled(summary, styles::tool_style()),
            ]));

            // For Edit, show the diff
            if name == "Edit" {
                if let Some(ref v) = parsed {
                    render_edit_diff(lines, v);
                }
            }
        }
        ConversationContent::ToolResult { .. } => {
            // Don't show tool results by default - too verbose
            // The tool use line already indicates what happened
        }
        _ => {}
    }
}

/// Shorten a file path for display
fn shorten_path(path: &str) -> String {
    // Remove common prefixes
    let shortened = path
        .strip_prefix("/Users/chris/git/")
        .or_else(|| path.strip_prefix("/Users/chris/"))
        .or_else(|| path.strip_prefix("/home/"))
        .unwrap_or(path);
    shortened.to_string()
}

/// Render an edit diff from the Edit tool input
fn render_edit_diff(lines: &mut Vec<Line>, input: &serde_json::Value) {
    let old_string = input.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
    let new_string = input.get("new_string").and_then(|v| v.as_str()).unwrap_or("");

    if old_string.is_empty() && new_string.is_empty() {
        return;
    }

    // Show removed lines (red)
    for line in old_string.lines().take(10) {
        let display = if line.len() > 80 {
            format!("  - {}...", &line[..80])
        } else {
            format!("  - {}", line)
        };
        lines.push(Line::from(Span::styled(display, styles::diff_removed_style())));
    }
    if old_string.lines().count() > 10 {
        lines.push(Line::from(Span::styled("  - ...", styles::diff_removed_style())));
    }

    // Show added lines (green)
    for line in new_string.lines().take(10) {
        let display = if line.len() > 80 {
            format!("  + {}...", &line[..80])
        } else {
            format!("  + {}", line)
        };
        lines.push(Line::from(Span::styled(display, styles::diff_added_style())));
    }
    if new_string.lines().count() > 10 {
        lines.push(Line::from(Span::styled("  + ...", styles::diff_added_style())));
    }

    lines.push(Line::from(""));
}

fn render_bash_message(lines: &mut Vec<Line>, content: &ConversationContent, _show_details: bool) {
    if let ConversationContent::BashCommand { command, output: _, exit_code } = content {
        // Green or red dot based on exit code
        let (dot_style, status_style) = if *exit_code == 0 || *exit_code == -1 {
            (styles::dot_success(), styles::bash_style())
        } else {
            (styles::dot_error(), styles::error_style())
        };

        // Truncate long commands
        let display_cmd = if command.len() > 60 {
            format!("{}...", &command[..60])
        } else {
            command.clone()
        };

        lines.push(Line::from(vec![
            Span::styled(styles::DOT, dot_style),
            Span::styled(format!("$ {}", display_cmd), status_style),
        ]));

        // Only show exit code if non-zero (error)
        if *exit_code != 0 && *exit_code != -1 {
            lines.push(Line::from(Span::styled(
                format!("  exit code: {}", exit_code),
                styles::error_style(),
            )));
        }
    }
}
