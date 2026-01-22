//! Types for Claude CLI streaming JSON output

use serde::{Deserialize, Serialize};

/// Events emitted from parsing Claude CLI stream-json output
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text content (delta or full)
    Text(String),
    /// Tool use started
    ToolUse { name: String, input: String },
    /// Tool result received
    ToolResult { name: String, result: String },
    /// Thinking content
    Thinking(String),
    /// Token usage update
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
    },
}

/// Message role in conversation
#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeMessage {
    pub id: Option<String>,
    pub role: Option<String>,
    pub content: Option<Vec<ContentBlock>>,
    pub model: Option<String>,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}

/// Content block in a message
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(other)]
    Unknown,
}

/// Usage information
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

/// Root-level streaming JSON event from Claude CLI
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum RawStreamEvent {
    #[serde(rename = "system")]
    System {
        subtype: Option<String>,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(rename = "assistant")]
    Assistant { message: ClaudeMessage },
    #[serde(rename = "user")]
    User { message: ClaudeMessage },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u64,
        content_block: ContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u64,
        delta: ContentDelta,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u64 },
    #[serde(rename = "message_start")]
    MessageStart { message: ClaudeMessage },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDeltaData,
        usage: Option<Usage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "result")]
    Result {
        subtype: Option<String>,
        result: Option<serde_json::Value>,
        is_error: Option<bool>,
        #[serde(flatten)]
        data: serde_json::Value,
    },
    #[serde(other)]
    Unknown,
}

/// Delta for content blocks
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(other)]
    Unknown,
}

/// Delta for message-level updates
#[derive(Debug, Clone, Deserialize)]
pub struct MessageDeltaData {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}
