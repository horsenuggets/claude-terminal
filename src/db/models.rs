//! Database models for session storage

use serde::{Deserialize, Serialize};

/// Session record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSession {
    pub id: String,
    pub pid: u32,
    pub cwd: String,
    pub task: String,
    pub app: Option<String>,
    pub tmux_window: Option<String>,
    pub parent_id: Option<String>,
    pub model: Option<String>,
    pub token_count: i64,
    pub cost: f64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl DbSession {
    /// Create a new session with default values
    pub fn new(id: String, pid: u32, cwd: String, task: String) -> Self {
        Self {
            id,
            pid,
            cwd,
            task,
            app: Some("claude-terminal".to_string()),
            tmux_window: std::env::var("TMUX_PANE").ok(),
            parent_id: None,
            model: None,
            token_count: 0,
            cost: 0.0,
            created_at: 0,
            updated_at: 0,
        }
    }

    /// Create a child session
    pub fn child(id: String, parent_id: String, pid: u32, cwd: String, task: String) -> Self {
        Self {
            id,
            pid,
            cwd,
            task,
            app: Some("claude-terminal".to_string()),
            tmux_window: None,
            parent_id: Some(parent_id),
            model: None,
            token_count: 0,
            cost: 0.0,
            created_at: 0,
            updated_at: 0,
        }
    }
}

/// Message record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,           // "user", "assistant", "system", "tool"
    pub content: String,
    pub model: Option<String>,
    pub tool_calls: Option<String>,  // JSON array of tool calls
    pub created_at: i64,
}

impl DbMessage {
    /// Create a new user message
    pub fn user(session_id: String, content: String) -> Self {
        Self {
            id: 0,
            session_id,
            role: "user".to_string(),
            content,
            model: None,
            tool_calls: None,
            created_at: 0,
        }
    }

    /// Create a new assistant message
    pub fn assistant(session_id: String, content: String, model: Option<String>) -> Self {
        Self {
            id: 0,
            session_id,
            role: "assistant".to_string(),
            content,
            model,
            tool_calls: None,
            created_at: 0,
        }
    }

    /// Create a new tool message
    pub fn tool(session_id: String, content: String, tool_calls: Option<String>) -> Self {
        Self {
            id: 0,
            session_id,
            role: "tool".to_string(),
            content,
            model: None,
            tool_calls,
            created_at: 0,
        }
    }
}

/// File history record for tracking modifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbFileHistory {
    pub id: i64,
    pub session_id: String,
    pub path: String,
    pub content_before: Option<String>,
    pub content_after: Option<String>,
    pub message_id: Option<i64>,
    pub created_at: i64,
}

impl DbFileHistory {
    /// Create a new file history entry
    pub fn new(
        session_id: String,
        path: String,
        content_before: Option<String>,
        content_after: Option<String>,
    ) -> Self {
        Self {
            id: 0,
            session_id,
            path,
            content_before,
            content_after,
            message_id: None,
            created_at: 0,
        }
    }
}

/// Inbox message for cross-session communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbInboxMessage {
    pub id: i64,
    pub target_session_id: String,
    pub from_session_id: String,
    pub message: String,
    pub created_at: i64,
}
