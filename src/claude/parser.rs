//! Parser for Claude CLI stream-json output

use anyhow::Result;

use super::types::{ContentBlock, ContentDelta, RawStreamEvent, StreamEvent};

/// Parser state for accumulating tool use inputs
#[derive(Debug, Default)]
pub struct StreamParser {
    /// Current tool use being accumulated
    current_tool_name: Option<String>,
    current_tool_input: String,
}

impl StreamParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a line of JSON from Claude CLI stream output
    pub fn parse_line(&mut self, line: &str) -> Result<Vec<StreamEvent>> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(Vec::new());
        }

        let raw: RawStreamEvent = match serde_json::from_str(line) {
            Ok(event) => event,
            Err(e) => {
                tracing::debug!("Failed to parse stream event: {} - line: {}", e, line);
                return Ok(Vec::new());
            }
        };

        self.process_event(raw)
    }

    fn process_event(&mut self, event: RawStreamEvent) -> Result<Vec<StreamEvent>> {
        let mut events = Vec::new();

        match event {
            RawStreamEvent::Assistant { message } | RawStreamEvent::MessageStart { message } => {
                // Process any content blocks in the message
                if let Some(content) = message.content {
                    for block in content {
                        events.extend(self.process_content_block(block)?);
                    }
                }
                // Process usage
                if let Some(usage) = message.usage {
                    events.push(StreamEvent::Usage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_read_tokens: usage.cache_read_input_tokens,
                        cache_write_tokens: usage.cache_creation_input_tokens,
                    });
                }
            }
            RawStreamEvent::ContentBlockStart { content_block, .. } => {
                events.extend(self.process_content_block(content_block)?);
            }
            RawStreamEvent::ContentBlockDelta { delta, .. } => {
                match delta {
                    ContentDelta::TextDelta { text } => {
                        events.push(StreamEvent::Text(text));
                    }
                    ContentDelta::InputJsonDelta { partial_json } => {
                        // Accumulate tool input
                        self.current_tool_input.push_str(&partial_json);
                    }
                    ContentDelta::ThinkingDelta { thinking } => {
                        events.push(StreamEvent::Thinking(thinking));
                    }
                    ContentDelta::Unknown => {}
                }
            }
            RawStreamEvent::ContentBlockStop { .. } => {
                // Finalize tool use if we were accumulating one
                if let Some(name) = self.current_tool_name.take() {
                    let input = std::mem::take(&mut self.current_tool_input);
                    events.push(StreamEvent::ToolUse { name, input });
                }
            }
            RawStreamEvent::MessageDelta { usage, .. } => {
                if let Some(usage) = usage {
                    events.push(StreamEvent::Usage {
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        cache_read_tokens: usage.cache_read_input_tokens,
                        cache_write_tokens: usage.cache_creation_input_tokens,
                    });
                }
            }
            RawStreamEvent::Result { result, subtype, .. } => {
                // Handle tool results
                if subtype.as_deref() == Some("tool_result") {
                    if let Some(result_data) = result {
                        let result_str = if let Some(s) = result_data.as_str() {
                            s.to_string()
                        } else {
                            serde_json::to_string_pretty(&result_data).unwrap_or_default()
                        };
                        events.push(StreamEvent::ToolResult {
                            name: "tool".to_string(),
                            result: result_str,
                        });
                    }
                }
            }
            RawStreamEvent::System { .. }
            | RawStreamEvent::User { .. }
            | RawStreamEvent::MessageStop
            | RawStreamEvent::Unknown => {}
        }

        Ok(events)
    }

    fn process_content_block(&mut self, block: ContentBlock) -> Result<Vec<StreamEvent>> {
        let mut events = Vec::new();

        match block {
            ContentBlock::Text { text } => {
                events.push(StreamEvent::Text(text));
            }
            ContentBlock::ToolUse { name, input, .. } => {
                // Store the tool name, we'll emit the event when we get all the input
                self.current_tool_name = Some(name.clone());
                self.current_tool_input = serde_json::to_string_pretty(&input).unwrap_or_default();
                // If the input is already complete, emit now
                if !self.current_tool_input.is_empty() {
                    let input = std::mem::take(&mut self.current_tool_input);
                    self.current_tool_name = None;
                    events.push(StreamEvent::ToolUse { name, input });
                }
            }
            ContentBlock::ToolResult { content, .. } => {
                let result = if let Some(s) = content.as_str() {
                    s.to_string()
                } else {
                    serde_json::to_string_pretty(&content).unwrap_or_default()
                };
                events.push(StreamEvent::ToolResult {
                    name: "tool".to_string(),
                    result,
                });
            }
            ContentBlock::Thinking { thinking } => {
                events.push(StreamEvent::Thinking(thinking));
            }
            ContentBlock::Unknown => {}
        }

        Ok(events)
    }
}
