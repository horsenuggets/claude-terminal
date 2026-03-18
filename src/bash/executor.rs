//! Bash command executor
//!
//! Provides both persistent shell execution (where env vars and cwd persist) and
//! one-shot execution (new shell for each command).

use anyhow::Result;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{mpsc, Mutex},
};

use super::persistent_shell::PersistentShell;
use crate::app::AppMessage;

/// Executes bash commands and sends output to the app
pub struct BashExecutor {
    message_tx: mpsc::Sender<AppMessage>,
    persistent_shell: Arc<Mutex<Option<PersistentShell>>>,
    use_persistent: bool,
}

impl BashExecutor {
    pub fn new(message_tx: mpsc::Sender<AppMessage>) -> Self {
        Self {
            message_tx,
            persistent_shell: Arc::new(Mutex::new(None)),
            use_persistent: true,
        }
    }

    /// Initialize the persistent shell
    pub async fn init_persistent_shell(&self) -> Result<()> {
        let shell = PersistentShell::new().await?;
        let mut guard = self.persistent_shell.lock().await;
        *guard = Some(shell);
        Ok(())
    }

    /// Get current working directory from persistent shell
    pub async fn cwd(&self) -> Option<PathBuf> {
        let guard = self.persistent_shell.lock().await;
        guard.as_ref().map(|s| s.cwd().clone())
    }

    /// Execute a bash command using persistent shell if available
    pub async fn execute(&self, command: &str) -> Result<()> {
        if self.use_persistent {
            self.execute_persistent(command).await
        } else {
            self.execute_oneshot(command).await
        }
    }

    /// Execute using persistent shell (env vars and cwd persist)
    async fn execute_persistent(&self, command: &str) -> Result<()> {
        let tx = self.message_tx.clone();
        let shell = self.persistent_shell.clone();
        let command = command.to_string();

        tokio::spawn(async move {
            let mut guard = shell.lock().await;

            // Initialize shell if needed
            if guard.is_none() {
                match PersistentShell::new().await {
                    Ok(s) => *guard = Some(s),
                    Err(e) => {
                        let _ = tx
                            .send(AppMessage::BashOutput(format!(
                                "Error initializing shell: {}",
                                e
                            )))
                            .await;
                        let _ = tx.send(AppMessage::BashFinished(1)).await;
                        return;
                    }
                }
            }

            let shell = guard.as_mut().unwrap();

            // Check if shell is still alive
            if !shell.is_alive() {
                // Try to restart
                match PersistentShell::new().await {
                    Ok(s) => *guard = Some(s),
                    Err(e) => {
                        let _ = tx
                            .send(AppMessage::BashOutput(format!(
                                "Error restarting shell: {}",
                                e
                            )))
                            .await;
                        let _ = tx.send(AppMessage::BashFinished(1)).await;
                        return;
                    }
                }
            }

            let shell = guard.as_mut().unwrap();

            match shell.execute(&command).await {
                Ok(output) => {
                    // Combine stdout and stderr
                    let mut combined = output.stdout;
                    if !output.stderr.is_empty() {
                        if !combined.is_empty() {
                            combined.push('\n');
                        }
                        combined.push_str(&output.stderr);
                    }
                    let _ = tx.send(AppMessage::BashOutput(combined)).await;
                    let _ = tx.send(AppMessage::BashFinished(output.exit_code)).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AppMessage::BashOutput(format!("Error: {}", e)))
                        .await;
                    let _ = tx.send(AppMessage::BashFinished(1)).await;
                }
            }
        });

        Ok(())
    }

    /// Execute using one-shot shell (no state persistence)
    async fn execute_oneshot(&self, command: &str) -> Result<()> {
        let tx = self.message_tx.clone();
        let command = command.to_string();

        tokio::spawn(async move {
            let result = execute_command_oneshot(&command).await;
            match result {
                Ok((output, exit_code)) => {
                    let _ = tx.send(AppMessage::BashOutput(output)).await;
                    let _ = tx.send(AppMessage::BashFinished(exit_code)).await;
                }
                Err(e) => {
                    let _ = tx
                        .send(AppMessage::BashOutput(format!("Error: {}", e)))
                        .await;
                    let _ = tx.send(AppMessage::BashFinished(1)).await;
                }
            }
        });

        Ok(())
    }

    /// Kill the persistent shell
    pub async fn kill_shell(&self) -> Result<()> {
        let mut guard = self.persistent_shell.lock().await;
        if let Some(ref mut shell) = *guard {
            shell.kill().await?;
        }
        *guard = None;
        Ok(())
    }
}

async fn execute_command_oneshot(command: &str) -> Result<(String, i32)> {
    let mut child = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
    } else {
        Command::new("sh")
            .args(["-c", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut output = String::new();

    // Read stdout
    if let Some(stdout) = stdout {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).await? > 0 {
            output.push_str(&line);
            line.clear();
        }
    }

    // Read stderr
    if let Some(stderr) = stderr {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).await? > 0 {
            output.push_str(&line);
            line.clear();
        }
    }

    let status = child.wait().await?;
    let exit_code = status.code().unwrap_or(1);

    Ok((output, exit_code))
}
