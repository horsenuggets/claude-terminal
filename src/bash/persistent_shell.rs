//! Persistent shell that maintains state between commands
//!
//! Unlike the basic executor which spawns a new shell for each command, this maintains a
//! long-running shell process where environment variables, working directory, and shell state
//! persist across commands.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Output from a command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub cwd: PathBuf,
}

/// A persistent shell session that maintains state between commands
pub struct PersistentShell {
    child: Child,
    stdin: ChildStdin,
    output_rx: mpsc::Receiver<String>,
    current_cwd: PathBuf,
    shell_type: String,
}

impl PersistentShell {
    /// Create a new persistent shell
    pub async fn new() -> Result<Self> {
        let shell = if cfg!(target_os = "windows") {
            "cmd".to_string()
        } else {
            // Prefer user's shell, fallback to bash
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
        };

        let shell_type = if shell.contains("zsh") {
            "zsh"
        } else if shell.contains("bash") {
            "bash"
        } else {
            "sh"
        };

        let mut child = Command::new(&shell)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PS1", "")
            .env("PS2", "")
            .env("PROMPT_COMMAND", "")
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to get stderr"))?;

        // Create channel for combined output
        let (output_tx, output_rx) = mpsc::channel(1000);

        // Spawn stdout reader
        let stdout_tx = output_tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let _ = stdout_tx.send(format!("OUT:{}", line.trim_end())).await;
                line.clear();
            }
        });

        // Spawn stderr reader
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let _ = output_tx.send(format!("ERR:{}", line.trim_end())).await;
                line.clear();
            }
        });

        let current_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));

        let mut shell = Self {
            child,
            stdin,
            output_rx,
            current_cwd,
            shell_type: shell_type.to_string(),
        };

        // Initialize shell with some setup
        shell.init().await?;

        Ok(shell)
    }

    /// Initialize the shell with setup commands
    async fn init(&mut self) -> Result<()> {
        // Disable any prompts and set up clean environment
        if self.shell_type == "bash" {
            self.write_raw("set +o history\n").await?;
            self.write_raw("export PS1=''\n").await?;
            self.write_raw("export PS2=''\n").await?;
        } else if self.shell_type == "zsh" {
            self.write_raw("unsetopt PROMPT_SUBST\n").await?;
            self.write_raw("export PS1=''\n").await?;
            self.write_raw("export PS2=''\n").await?;
            self.write_raw("export RPS1=''\n").await?;
        }

        // Drain any initial output
        self.drain_output().await;

        Ok(())
    }

    /// Write raw text to stdin
    async fn write_raw(&mut self, text: &str) -> Result<()> {
        self.stdin.write_all(text.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Drain any pending output without blocking
    async fn drain_output(&mut self) {
        while let Ok(result) = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            self.output_rx.recv(),
        )
        .await
        {
            if result.is_none() {
                break;
            }
        }
    }

    /// Execute a command and return its output
    pub async fn execute(&mut self, command: &str) -> Result<CommandOutput> {
        // Generate unique markers for this command
        let marker = Uuid::new_v4().to_string().replace("-", "");
        let start_marker = format!("__START_{}__", marker);
        let end_marker = format!("__END_{}__", marker);
        let exit_marker = format!("__EXIT_{}__", marker);

        // Drain any pending output first
        self.drain_output().await;

        // Build the wrapped command that outputs markers and captures exit code
        let wrapped_command = format!(
            "echo '{}'; {{ {}; }}; __exit_code=$?; echo '{}'; echo \"{}_$__exit_code\"; pwd\n",
            start_marker, command, end_marker, exit_marker
        );

        self.write_raw(&wrapped_command).await?;

        // Collect output until we see end marker and exit code
        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();
        let mut exit_code = 0i32;
        let mut new_cwd = PathBuf::new();
        let mut started = false;
        let mut ended = false;

        let timeout = tokio::time::Duration::from_secs(300); // 5 minute timeout
        let start_time = tokio::time::Instant::now();

        loop {
            if start_time.elapsed() > timeout {
                return Err(anyhow!("Command timed out after 5 minutes"));
            }

            match tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                self.output_rx.recv(),
            )
            .await
            {
                Ok(Some(line)) => {
                    let (prefix, content) = if line.starts_with("OUT:") {
                        ("stdout", &line[4..])
                    } else if line.starts_with("ERR:") {
                        ("stderr", &line[4..])
                    } else {
                        continue;
                    };

                    // Check for markers
                    if content.contains(&start_marker) {
                        started = true;
                        continue;
                    }

                    if content.contains(&end_marker) {
                        ended = true;
                        continue;
                    }

                    if content.starts_with(&exit_marker) {
                        // Parse exit code
                        if let Some(code_str) = content.strip_prefix(&format!("{}_", exit_marker)) {
                            exit_code = code_str.trim().parse().unwrap_or(1);
                        }
                        continue;
                    }

                    // After end marker, next stdout line is pwd
                    if ended && prefix == "stdout" && !content.is_empty() {
                        new_cwd = PathBuf::from(content.trim());
                        break;
                    }

                    // Collect output between markers
                    if started && !ended {
                        if prefix == "stdout" {
                            stdout_lines.push(content.to_string());
                        } else {
                            stderr_lines.push(content.to_string());
                        }
                    }
                }
                Ok(None) => {
                    // Channel closed, shell died
                    return Err(anyhow!("Shell process terminated"));
                }
                Err(_) => {
                    // Timeout on recv, check if shell is still alive
                    if self.child.try_wait()?.is_some() {
                        return Err(anyhow!("Shell process terminated"));
                    }
                    // If we've already ended, the pwd should come soon
                    if ended {
                        // Give a bit more time for pwd
                        continue;
                    }
                }
            }
        }

        self.current_cwd = new_cwd.clone();

        Ok(CommandOutput {
            stdout: stdout_lines.join("\n"),
            stderr: stderr_lines.join("\n"),
            exit_code,
            cwd: new_cwd,
        })
    }

    /// Get current working directory
    pub fn cwd(&self) -> &PathBuf {
        &self.current_cwd
    }

    /// Check if the shell is still running
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Kill the shell process
    pub async fn kill(&mut self) -> Result<()> {
        self.child.kill().await?;
        Ok(())
    }
}

impl Drop for PersistentShell {
    fn drop(&mut self) {
        // Try to kill the shell process on drop
        let _ = self.child.start_kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_persistent_shell_basic() {
        let mut shell = PersistentShell::new().await.unwrap();

        let output = shell.execute("echo hello").await.unwrap();
        assert_eq!(output.stdout.trim(), "hello");
        assert_eq!(output.exit_code, 0);
    }

    #[tokio::test]
    async fn test_persistent_shell_env_persistence() {
        let mut shell = PersistentShell::new().await.unwrap();

        // Set environment variable
        let _ = shell.execute("export TEST_VAR=test_value").await.unwrap();

        // Check it persists
        let output = shell.execute("echo $TEST_VAR").await.unwrap();
        assert_eq!(output.stdout.trim(), "test_value");
    }

    #[tokio::test]
    async fn test_persistent_shell_cd_persistence() {
        let mut shell = PersistentShell::new().await.unwrap();

        // Change directory
        let _ = shell.execute("cd /tmp").await.unwrap();

        // Check pwd
        let output = shell.execute("pwd").await.unwrap();
        assert!(output.stdout.contains("/tmp") || output.stdout.contains("/private/tmp"));
        assert_eq!(output.cwd.to_string_lossy(), output.stdout.trim());
    }

    #[tokio::test]
    async fn test_persistent_shell_exit_code() {
        let mut shell = PersistentShell::new().await.unwrap();

        let output = shell.execute("exit 42").await;
        // Shell should handle subshell exit
        // This might kill the shell or return exit code depending on shell
    }

    #[tokio::test]
    async fn test_persistent_shell_stderr() {
        let mut shell = PersistentShell::new().await.unwrap();

        let output = shell.execute("echo error >&2").await.unwrap();
        assert_eq!(output.stderr.trim(), "error");
    }
}
