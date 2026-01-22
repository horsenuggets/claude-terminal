//! Session manager for claude-sessions integration

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::{
    fs,
    sync::mpsc,
};

use crate::app::AppMessage;

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub pid: u32,
    pub cwd: String,
    pub task: String,
    pub started: DateTime<Utc>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub tmux_window: Option<String>,
}

/// Incoming message from another session
#[derive(Debug, Clone, Deserialize)]
pub struct SessionMessage {
    pub from: String,
    pub message: String,
    pub time: String,
}

/// Manages interaction with the claude-sessions system
pub struct SessionManager {
    message_tx: mpsc::Sender<AppMessage>,
    sessions_dir: PathBuf,
    session_id: Option<String>,
}

impl SessionManager {
    pub fn new(message_tx: mpsc::Sender<AppMessage>) -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let sessions_dir = home.join(".claude-sessions");

        // Create directories if they don't exist
        std::fs::create_dir_all(&sessions_dir)?;
        std::fs::create_dir_all(sessions_dir.join("messages"))?;

        Ok(Self {
            message_tx,
            sessions_dir,
            session_id: None,
        })
    }

    /// Register this session
    pub async fn register(&mut self, task: &str) -> Result<String> {
        let pid = std::process::id();
        let timestamp = Utc::now().timestamp();
        let session_id = format!("claude-terminal-{}-{}", pid, timestamp);

        let info = SessionInfo {
            id: session_id.clone(),
            pid,
            cwd: std::env::current_dir()?.to_string_lossy().to_string(),
            task: task.to_string(),
            started: Utc::now(),
            app: Some("claude-terminal".to_string()),
            tmux_window: std::env::var("TMUX_PANE").ok(),
        };

        let path = self.sessions_dir.join(format!("{}.json", session_id));
        let json = serde_json::to_string_pretty(&info)?;
        fs::write(&path, json).await?;

        self.session_id = Some(session_id.clone());

        // Start polling for messages
        self.start_message_polling();

        Ok(session_id)
    }

    /// Deregister this session
    pub async fn deregister(&self) -> Result<()> {
        if let Some(ref session_id) = self.session_id {
            let path = self.sessions_dir.join(format!("{}.json", session_id));
            let _ = fs::remove_file(&path).await;

            let msg_path = self.sessions_dir.join("messages").join(session_id);
            let _ = fs::remove_file(&msg_path).await;
        }
        Ok(())
    }

    /// List active sessions (excluding self)
    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let mut sessions = Vec::new();
        let mut entries = fs::read_dir(&self.sessions_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path).await {
                    if let Ok(info) = serde_json::from_str::<SessionInfo>(&content) {
                        // Skip self
                        if Some(&info.id) == self.session_id.as_ref() {
                            continue;
                        }
                        // Check if process is still alive
                        if is_process_alive(info.pid) {
                            sessions.push(info);
                        } else {
                            // Clean up stale session file
                            let _ = fs::remove_file(&path).await;
                        }
                    }
                }
            }
        }

        Ok(sessions)
    }

    /// Send message to a specific session
    pub async fn send_message(&self, target_id: &str, message: &str) -> Result<()> {
        let from = self.session_id.as_ref().map_or("unknown", |s| s.as_str());
        let msg = serde_json::json!({
            "from": from,
            "message": message,
            "time": Utc::now().to_rfc3339()
        });

        let path = self.sessions_dir.join("messages").join(target_id);
        let mut content = String::new();

        // Append to existing messages
        if path.exists() {
            content = fs::read_to_string(&path).await.unwrap_or_default();
        }
        content.push_str(&serde_json::to_string(&msg)?);
        content.push('\n');

        fs::write(&path, content).await?;
        Ok(())
    }

    /// Broadcast message to all sessions
    pub async fn broadcast(&self, message: &str) -> Result<()> {
        let sessions = self.list_sessions().await?;
        for session in sessions {
            self.send_message(&session.id, message).await?;
        }
        Ok(())
    }

    /// Read and clear inbox
    pub async fn read_inbox(&self) -> Result<Vec<SessionMessage>> {
        let Some(ref session_id) = self.session_id else {
            return Ok(Vec::new());
        };

        let path = self.sessions_dir.join("messages").join(session_id);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&path).await?;
        let messages: Vec<SessionMessage> = content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        // Clear the inbox
        fs::remove_file(&path).await?;

        Ok(messages)
    }

    /// Start background task to poll for messages
    fn start_message_polling(&self) {
        let session_id = match &self.session_id {
            Some(id) => id.clone(),
            None => return,
        };
        let sessions_dir = self.sessions_dir.clone();
        let tx = self.message_tx.clone();

        tokio::spawn(async move {
            let path = sessions_dir.join("messages").join(&session_id);
            let mut last_size = 0u64;

            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                if let Ok(metadata) = tokio::fs::metadata(&path).await {
                    let size = metadata.len();
                    if size > last_size {
                        // New messages
                        if let Ok(content) = tokio::fs::read_to_string(&path).await {
                            let messages: Vec<SessionMessage> = content
                                .lines()
                                .filter_map(|line| serde_json::from_str(line).ok())
                                .collect();

                            for msg in messages {
                                let _ = tx
                                    .send(AppMessage::SessionMessage {
                                        from: msg.from,
                                        message: msg.message,
                                    })
                                    .await;
                            }

                            // Clear after reading
                            let _ = tokio::fs::remove_file(&path).await;
                            last_size = 0;
                        }
                    }
                    last_size = size;
                }
            }
        });
    }
}

/// Check if a process is alive
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            libc::kill(pid as i32, 0) == 0
        }
    }
    #[cfg(windows)]
    {
        // On Windows, try to open the process
        use std::process::Command;
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}
