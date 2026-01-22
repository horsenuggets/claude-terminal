//! Main application state and event loop

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::{
    bash::BashExecutor,
    claude::{ClaudeProcess, StreamEvent},
    sessions::SessionManager,
    ui::{self, InputMode, RenderState},
    voice::VoiceRecorder,
};

/// Messages that can be sent to the app from various sources
#[derive(Debug)]
pub enum AppMessage {
    /// Claude sent a streaming event
    ClaudeEvent(StreamEvent),
    /// Claude process finished
    ClaudeFinished,
    /// Claude process error
    ClaudeError(String),
    /// Bash command output
    BashOutput(String),
    /// Bash command finished
    BashFinished(i32),
    /// Voice transcription result
    VoiceTranscription(String),
    /// Voice recording error
    VoiceError(String),
    /// Session message received
    SessionMessage { from: String, message: String },
}

/// Application state
pub struct App {
    /// Terminal handle
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Current model
    model: String,
    /// Continue previous session
    continue_session: bool,
    /// Resume specific session
    resume_session: Option<String>,
    /// Session ID for this instance
    session_id: Option<String>,
    /// Conversation history for display
    messages: Vec<ConversationEntry>,
    /// Current input text
    input: String,
    /// Input cursor position
    cursor_position: usize,
    /// Input mode (normal, recording)
    input_mode: InputMode,
    /// Message queue (for sending while Claude is busy)
    message_queue: Vec<String>,
    /// Is Claude currently processing?
    claude_busy: bool,
    /// Current streaming text buffer
    streaming_buffer: String,
    /// Claude process handle
    claude_process: Option<ClaudeProcess>,
    /// Bash executor
    bash_executor: BashExecutor,
    /// Voice recorder
    voice_recorder: VoiceRecorder,
    /// Session manager
    session_manager: SessionManager,
    /// App message receiver
    message_rx: mpsc::Receiver<AppMessage>,
    /// App message sender (shared)
    message_tx: mpsc::Sender<AppMessage>,
    /// Scroll offset for conversation view
    scroll_offset: usize,
    /// Input history
    input_history: Vec<String>,
    /// Current position in input history
    history_index: Option<usize>,
    /// Should quit
    should_quit: bool,
    /// Status message
    status_message: Option<String>,
    /// Token usage tracking
    token_usage: TokenUsage,
}

/// A single entry in the conversation
#[derive(Debug, Clone)]
pub struct ConversationEntry {
    pub role: Role,
    pub content: ConversationContent,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
    Bash,
}

#[derive(Debug, Clone)]
pub enum ConversationContent {
    Text(String),
    ToolUse { name: String, input: String },
    ToolResult { name: String, result: String },
    Thinking(String),
    BashCommand { command: String, output: String, exit_code: i32 },
}

#[derive(Debug, Default, Clone)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl App {
    pub fn new(model: String, continue_session: bool, resume_session: Option<String>) -> Result<Self> {
        // Set up terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        // Create message channel
        let (message_tx, message_rx) = mpsc::channel(100);

        // Initialize components
        let bash_executor = BashExecutor::new(message_tx.clone());
        let voice_recorder = VoiceRecorder::new(message_tx.clone());
        let session_manager = SessionManager::new(message_tx.clone())?;

        Ok(Self {
            terminal,
            model,
            continue_session,
            resume_session,
            session_id: None,
            messages: Vec::new(),
            input: String::new(),
            cursor_position: 0,
            input_mode: InputMode::Normal,
            message_queue: Vec::new(),
            claude_busy: false,
            streaming_buffer: String::new(),
            claude_process: None,
            bash_executor,
            voice_recorder,
            session_manager,
            message_rx,
            message_tx,
            scroll_offset: 0,
            input_history: Vec::new(),
            history_index: None,
            should_quit: false,
            status_message: None,
            token_usage: TokenUsage::default(),
        })
    }

    /// Main event loop
    pub async fn run(&mut self) -> Result<()> {
        // Register with session manager
        self.session_id = Some(self.session_manager.register("interactive").await?);

        loop {
            // Draw UI
            self.draw()?;

            // Handle events with timeout
            tokio::select! {
                // Check for terminal events
                _ = tokio::time::sleep(Duration::from_millis(16)) => {
                    if event::poll(Duration::from_millis(0))? {
                        if let Event::Key(key) = event::read()? {
                            self.handle_key_event(key).await?;
                        }
                    }
                }

                // Check for app messages
                Some(msg) = self.message_rx.recv() => {
                    self.handle_app_message(msg).await?;
                }
            }

            if self.should_quit {
                break;
            }
        }

        // Cleanup
        self.cleanup()?;
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        // Extract state for rendering
        let state = RenderState {
            messages: &self.messages,
            input: &self.input,
            cursor_position: self.cursor_position,
            input_mode: self.input_mode,
            claude_busy: self.claude_busy,
            streaming_buffer: &self.streaming_buffer,
            model: &self.model,
            scroll_offset: self.scroll_offset,
            status_message: self.status_message.as_deref(),
            token_usage: &self.token_usage,
            message_queue_len: self.message_queue.len(),
        };

        self.terminal.draw(|frame| {
            ui::draw(frame, &state);
        })?;
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        match self.input_mode {
            InputMode::Normal => self.handle_normal_mode_key(key).await?,
            InputMode::Recording => self.handle_recording_mode_key(key).await?,
        }
        Ok(())
    }

    async fn handle_normal_mode_key(&mut self, key: KeyEvent) -> Result<()> {
        match (key.modifiers, key.code) {
            // Quit
            (KeyModifiers::CONTROL, KeyCode::Char('q')) => {
                self.should_quit = true;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                // Interrupt Claude if busy
                if self.claude_busy {
                    if let Some(ref mut process) = self.claude_process {
                        process.abort().await;
                        self.claude_busy = false;
                        self.status_message = Some("Interrupted".to_string());
                    }
                } else {
                    // Clear input if not busy
                    self.input.clear();
                    self.cursor_position = 0;
                }
            }
            // Submit input
            (_, KeyCode::Enter) => {
                if !self.input.is_empty() {
                    self.submit_input().await?;
                }
            }
            // Voice toggle
            (_, KeyCode::Char('*')) if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.toggle_voice_recording().await?;
            }
            // Character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            // Backspace
            (_, KeyCode::Backspace) => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input.remove(self.cursor_position);
                }
            }
            // Delete
            (_, KeyCode::Delete) => {
                if self.cursor_position < self.input.len() {
                    self.input.remove(self.cursor_position);
                }
            }
            // Cursor movement
            (_, KeyCode::Left) => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            (_, KeyCode::Right) => {
                if self.cursor_position < self.input.len() {
                    self.cursor_position += 1;
                }
            }
            (_, KeyCode::Home) => {
                self.cursor_position = 0;
            }
            (_, KeyCode::End) => {
                self.cursor_position = self.input.len();
            }
            // History navigation
            (_, KeyCode::Up) => {
                self.navigate_history(-1);
            }
            (_, KeyCode::Down) => {
                self.navigate_history(1);
            }
            // Scroll conversation
            (_, KeyCode::PageUp) => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
            }
            (_, KeyCode::PageDown) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_recording_mode_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Stop recording
            KeyCode::Char('*') => {
                self.toggle_voice_recording().await?;
            }
            // Cancel recording
            KeyCode::Esc => {
                self.voice_recorder.cancel().await;
                self.input_mode = InputMode::Normal;
                self.status_message = Some("Recording cancelled".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    async fn submit_input(&mut self) -> Result<()> {
        let input = std::mem::take(&mut self.input);
        self.cursor_position = 0;

        // Save to history
        if !input.is_empty() {
            self.input_history.push(input.clone());
            self.history_index = None;
        }

        // Check for commands
        if input.starts_with('!') {
            // Bash command
            let command = input[1..].trim();
            self.execute_bash(command).await?;
        } else if input.starts_with('/') {
            // Slash command
            self.handle_slash_command(&input).await?;
        } else {
            // Regular message to Claude
            self.send_to_claude(&input).await?;
        }

        Ok(())
    }

    async fn execute_bash(&mut self, command: &str) -> Result<()> {
        // Add to conversation
        self.messages.push(ConversationEntry {
            role: Role::Bash,
            content: ConversationContent::Text(format!("$ {}", command)),
            timestamp: chrono::Utc::now(),
        });

        self.bash_executor.execute(command).await?;
        Ok(())
    }

    async fn handle_slash_command(&mut self, input: &str) -> Result<()> {
        let parts: Vec<&str> = input[1..].splitn(2, ' ').collect();
        let command = parts[0];
        let args = parts.get(1).copied().unwrap_or("");

        match command {
            "quit" | "q" => {
                self.should_quit = true;
            }
            "clear" => {
                self.messages.clear();
                self.scroll_offset = 0;
            }
            "model" => {
                if !args.is_empty() {
                    self.model = args.to_string();
                    self.status_message = Some(format!("Model set to: {}", self.model));
                } else {
                    self.status_message = Some(format!("Current model: {}", self.model));
                }
            }
            "sessions" => {
                let sessions = self.session_manager.list_sessions().await?;
                let msg = if sessions.is_empty() {
                    "No other active sessions".to_string()
                } else {
                    sessions
                        .iter()
                        .map(|s| format!("  {} ({}) - {}", s.id, s.cwd, s.task))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                self.messages.push(ConversationEntry {
                    role: Role::System,
                    content: ConversationContent::Text(format!("Active sessions:\n{}", msg)),
                    timestamp: chrono::Utc::now(),
                });
            }
            "send" => {
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    self.session_manager.send_message(parts[0], parts[1]).await?;
                    self.status_message = Some(format!("Sent to {}", parts[0]));
                } else {
                    self.status_message = Some("Usage: /send <session-id> <message>".to_string());
                }
            }
            "broadcast" => {
                if !args.is_empty() {
                    self.session_manager.broadcast(args).await?;
                    self.status_message = Some("Broadcast sent".to_string());
                } else {
                    self.status_message = Some("Usage: /broadcast <message>".to_string());
                }
            }
            "inbox" => {
                let messages = self.session_manager.read_inbox().await?;
                if messages.is_empty() {
                    self.status_message = Some("No messages".to_string());
                } else {
                    for msg in messages {
                        self.messages.push(ConversationEntry {
                            role: Role::System,
                            content: ConversationContent::Text(format!(
                                "[{}] {}: {}",
                                msg.time, msg.from, msg.message
                            )),
                            timestamp: chrono::Utc::now(),
                        });
                    }
                }
            }
            "help" => {
                let help = r#"Commands:
  !<cmd>         Run bash command
  /quit          Exit
  /clear         Clear conversation
  /model <name>  Set model
  /sessions      List active sessions
  /send <id> <m> Send message to session
  /broadcast <m> Broadcast to all sessions
  /inbox         Read incoming messages
  *              Toggle voice recording
  Ctrl+C         Interrupt Claude
  Ctrl+Q         Quit"#;
                self.messages.push(ConversationEntry {
                    role: Role::System,
                    content: ConversationContent::Text(help.to_string()),
                    timestamp: chrono::Utc::now(),
                });
            }
            _ => {
                self.status_message = Some(format!("Unknown command: /{}", command));
            }
        }
        Ok(())
    }

    async fn send_to_claude(&mut self, message: &str) -> Result<()> {
        // If Claude is busy, queue the message
        if self.claude_busy {
            self.message_queue.push(message.to_string());
            self.status_message = Some(format!("Queued ({} pending)", self.message_queue.len()));
            return Ok(());
        }

        // Add user message to conversation
        self.messages.push(ConversationEntry {
            role: Role::User,
            content: ConversationContent::Text(message.to_string()),
            timestamp: chrono::Utc::now(),
        });

        // Build context from recent bash commands
        let context = self.build_context();

        // Start Claude process
        self.claude_busy = true;
        self.streaming_buffer.clear();

        let mut process = ClaudeProcess::new(
            &self.model,
            self.message_tx.clone(),
            self.continue_session,
            self.resume_session.take(),
        )?;

        let full_message = if context.is_empty() {
            message.to_string()
        } else {
            format!("{}\n\n{}", context, message)
        };

        process.send(&full_message).await?;
        self.claude_process = Some(process);

        // Reset scroll to see new messages
        self.scroll_offset = 0;

        Ok(())
    }

    /// Build context from recent bash commands to include with message
    fn build_context(&self) -> String {
        let recent_bash: Vec<_> = self
            .messages
            .iter()
            .rev()
            .take(5)
            .filter_map(|m| match &m.content {
                ConversationContent::BashCommand {
                    command,
                    output,
                    exit_code,
                } => Some(format!(
                    "$ {}\n{}\n(exit code: {})",
                    command, output, exit_code
                )),
                _ => None,
            })
            .collect();

        if recent_bash.is_empty() {
            String::new()
        } else {
            format!(
                "[Recent terminal activity]\n{}\n",
                recent_bash.into_iter().rev().collect::<Vec<_>>().join("\n\n")
            )
        }
    }

    async fn toggle_voice_recording(&mut self) -> Result<()> {
        match self.input_mode {
            InputMode::Normal => {
                self.voice_recorder.start().await?;
                self.input_mode = InputMode::Recording;
                self.status_message = Some("Recording...".to_string());
            }
            InputMode::Recording => {
                self.voice_recorder.stop().await?;
                self.input_mode = InputMode::Normal;
                self.status_message = Some("Transcribing...".to_string());
            }
        }
        Ok(())
    }

    fn navigate_history(&mut self, direction: i32) {
        if self.input_history.is_empty() {
            return;
        }

        let new_index = match self.history_index {
            None if direction < 0 => Some(self.input_history.len() - 1),
            Some(i) if direction < 0 && i > 0 => Some(i - 1),
            Some(i) if direction > 0 && i < self.input_history.len() - 1 => Some(i + 1),
            Some(_) if direction > 0 => None,
            idx => idx,
        };

        self.history_index = new_index;
        self.input = match new_index {
            Some(i) => self.input_history[i].clone(),
            None => String::new(),
        };
        self.cursor_position = self.input.len();
    }

    async fn handle_app_message(&mut self, msg: AppMessage) -> Result<()> {
        match msg {
            AppMessage::ClaudeEvent(event) => {
                self.handle_claude_event(event);
            }
            AppMessage::ClaudeFinished => {
                self.claude_busy = false;
                self.claude_process = None;

                // Finalize streaming buffer
                if !self.streaming_buffer.is_empty() {
                    self.messages.push(ConversationEntry {
                        role: Role::Assistant,
                        content: ConversationContent::Text(std::mem::take(&mut self.streaming_buffer)),
                        timestamp: chrono::Utc::now(),
                    });
                }

                // Process queued messages
                if let Some(queued) = self.message_queue.pop() {
                    self.status_message = Some(format!("{} more queued", self.message_queue.len()));
                    // Use Box::pin to allow recursion in async
                    Box::pin(self.send_to_claude(&queued)).await?;
                }
            }
            AppMessage::ClaudeError(err) => {
                self.claude_busy = false;
                self.claude_process = None;
                self.messages.push(ConversationEntry {
                    role: Role::System,
                    content: ConversationContent::Text(format!("Error: {}", err)),
                    timestamp: chrono::Utc::now(),
                });
            }
            AppMessage::BashOutput(output) => {
                // Update the last bash entry with output
                if let Some(entry) = self.messages.last_mut() {
                    if let ConversationContent::Text(text) = &entry.content {
                        if text.starts_with("$ ") {
                            let command = text[2..].to_string();
                            entry.content = ConversationContent::BashCommand {
                                command,
                                output,
                                exit_code: 0,
                            };
                        }
                    }
                }
            }
            AppMessage::BashFinished(exit_code) => {
                // Update exit code
                if let Some(entry) = self.messages.last_mut() {
                    if let ConversationContent::BashCommand {
                        exit_code: ref mut ec,
                        ..
                    } = entry.content
                    {
                        *ec = exit_code;
                    }
                }
            }
            AppMessage::VoiceTranscription(text) => {
                // Insert transcription into input
                self.input.push_str(&text);
                self.cursor_position = self.input.len();
                self.status_message = Some("Transcription complete".to_string());
            }
            AppMessage::VoiceError(err) => {
                self.input_mode = InputMode::Normal;
                self.status_message = Some(format!("Voice error: {}", err));
            }
            AppMessage::SessionMessage { from, message } => {
                self.messages.push(ConversationEntry {
                    role: Role::System,
                    content: ConversationContent::Text(format!("[Session {}]: {}", from, message)),
                    timestamp: chrono::Utc::now(),
                });
            }
        }
        Ok(())
    }

    fn handle_claude_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::Text(text) => {
                self.streaming_buffer.push_str(&text);
            }
            StreamEvent::ToolUse { name, input } => {
                // Finalize any pending text
                if !self.streaming_buffer.is_empty() {
                    self.messages.push(ConversationEntry {
                        role: Role::Assistant,
                        content: ConversationContent::Text(std::mem::take(&mut self.streaming_buffer)),
                        timestamp: chrono::Utc::now(),
                    });
                }
                self.messages.push(ConversationEntry {
                    role: Role::Tool,
                    content: ConversationContent::ToolUse { name, input },
                    timestamp: chrono::Utc::now(),
                });
            }
            StreamEvent::ToolResult { name, result } => {
                self.messages.push(ConversationEntry {
                    role: Role::Tool,
                    content: ConversationContent::ToolResult { name, result },
                    timestamp: chrono::Utc::now(),
                });
            }
            StreamEvent::Thinking(text) => {
                self.messages.push(ConversationEntry {
                    role: Role::Assistant,
                    content: ConversationContent::Thinking(text),
                    timestamp: chrono::Utc::now(),
                });
            }
            StreamEvent::Usage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
            } => {
                self.token_usage.input_tokens += input_tokens;
                self.token_usage.output_tokens += output_tokens;
                self.token_usage.cache_read_tokens += cache_read_tokens;
                self.token_usage.cache_write_tokens += cache_write_tokens;
            }
        }
    }

    fn cleanup(&mut self) -> Result<()> {
        // Deregister session
        if let Some(session_id) = &self.session_id {
            // Blocking cleanup since we're exiting
            let _ = std::fs::remove_file(format!(
                "{}/.claude-sessions/{}.json",
                dirs::home_dir().unwrap().display(),
                session_id
            ));
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}
