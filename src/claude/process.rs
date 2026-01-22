//! Claude CLI process management

use anyhow::Result;
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
};

use crate::app::AppMessage;

use super::{StreamEvent, StreamParser};

/// Manages a Claude CLI process
pub struct ClaudeProcess {
    child: Child,
    message_tx: mpsc::Sender<AppMessage>,
    aborted: bool,
}

impl ClaudeProcess {
    /// Start a new Claude process
    pub fn new(
        model: &str,
        message_tx: mpsc::Sender<AppMessage>,
        continue_session: bool,
        resume_session: Option<String>,
    ) -> Result<Self> {
        let mut cmd = Command::new("claude");

        // Always use print mode with streaming JSON
        cmd.arg("--print");
        cmd.arg("--output-format");
        cmd.arg("stream-json");
        cmd.arg("--dangerously-skip-permissions");
        cmd.arg("--model");
        cmd.arg(model);

        // Session handling
        if let Some(session_id) = resume_session {
            cmd.arg("--resume");
            cmd.arg(session_id);
        } else if continue_session {
            cmd.arg("--continue");
        }

        // Set up stdio
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn()?;

        Ok(Self {
            child,
            message_tx,
            aborted: false,
        })
    }

    /// Send a message to Claude and start streaming the response
    pub async fn send(&mut self, message: &str) -> Result<()> {
        // Write message to stdin
        if let Some(ref mut stdin) = self.child.stdin {
            stdin.write_all(message.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        // Take stdin to close it (signals end of input)
        drop(self.child.stdin.take());

        // Spawn task to read stdout
        let stdout = self.child.stdout.take();
        let tx = self.message_tx.clone();

        if let Some(stdout) = stdout {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut parser = StreamParser::new();
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            match parser.parse_line(&line) {
                                Ok(events) => {
                                    for event in events {
                                        if tx.send(AppMessage::ClaudeEvent(event)).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::debug!("Parse error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(AppMessage::ClaudeError(e.to_string())).await;
                            break;
                        }
                    }
                }

                let _ = tx.send(AppMessage::ClaudeFinished).await;
            });
        }

        // Spawn task to read stderr
        let stderr = self.child.stderr.take();
        let tx_err = self.message_tx.clone();

        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                tracing::debug!("Claude stderr: {}", trimmed);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        Ok(())
    }

    /// Abort the Claude process
    pub async fn abort(&mut self) {
        if !self.aborted {
            self.aborted = true;
            let _ = self.child.kill().await;
        }
    }
}

impl Drop for ClaudeProcess {
    fn drop(&mut self) {
        // Try to kill the process if still running
        let _ = self.child.start_kill();
    }
}
