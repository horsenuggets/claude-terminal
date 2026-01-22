//! Bash command executor

use anyhow::Result;
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
};

use crate::app::AppMessage;

/// Executes bash commands and sends output to the app
pub struct BashExecutor {
    message_tx: mpsc::Sender<AppMessage>,
}

impl BashExecutor {
    pub fn new(message_tx: mpsc::Sender<AppMessage>) -> Self {
        Self { message_tx }
    }

    /// Execute a bash command
    pub async fn execute(&self, command: &str) -> Result<()> {
        let tx = self.message_tx.clone();
        let command = command.to_string();

        tokio::spawn(async move {
            let result = execute_command(&command).await;
            match result {
                Ok((output, exit_code)) => {
                    let _ = tx.send(AppMessage::BashOutput(output)).await;
                    let _ = tx.send(AppMessage::BashFinished(exit_code)).await;
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::BashOutput(format!("Error: {}", e))).await;
                    let _ = tx.send(AppMessage::BashFinished(1)).await;
                }
            }
        });

        Ok(())
    }
}

async fn execute_command(command: &str) -> Result<(String, i32)> {
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
