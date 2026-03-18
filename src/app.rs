//! Main application state and event loop

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, stdout};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::{
    bash::BashExecutor,
    claude::{ClaudeProcess, StreamEvent},
    commands::CommandManager,
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

/// Token usage tracking
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// A single entry in the conversation
#[derive(Debug, Clone)]
pub struct ConversationEntry {
    pub role: Role,
    pub content: ConversationContent,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq)]
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
    Error(String),
}

/// Application state
pub struct App {
    /// Current model
    model: String,
    /// Continue previous session
    continue_session: bool,
    /// Resume specific session
    resume_session: Option<String>,
    /// Session ID for this instance
    session_id: Option<String>,
    /// Conversation history
    messages: Vec<ConversationEntry>,
    /// Current input text
    input: String,
    /// Input cursor position
    cursor_position: usize,
    /// Current input mode
    input_mode: InputMode,
    /// Message queue (for sending while Claude is busy)
    message_queue: Vec<String>,
    /// Is Claude currently processing?
    claude_busy: bool,
    /// Buffer for streaming text
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
    /// Input history
    input_history: Vec<String>,
    /// Current position in input history
    history_index: Option<usize>,
    /// Scroll offset for conversation
    scroll_offset: usize,
    /// Should quit
    should_quit: bool,
    /// Token usage tracking
    token_usage: TokenUsage,
    /// Status message to display
    status_message: Option<String>,
    /// Command manager for autocomplete
    command_manager: CommandManager,
    /// Current autocomplete suggestion
    autocomplete_suggestion: Option<String>,
    /// Animation tick counter
    animation_tick: u8,
    /// Attached images (paths)
    attached_images: Vec<String>,
    /// Dynamic context for Whisper transcription (session-specific terms)
    whisper_dynamic_context: String,
    /// Auto-continue mode (ralph-style) - keeps working until task is complete
    auto_continue: bool,
    /// Count of consecutive auto-continues (for circuit breaker)
    auto_continue_count: u32,
    /// Maximum auto-continues before requiring user input (circuit breaker)
    max_auto_continues: u32,
    /// Verbose mode - show full tool details instead of summary
    verbose_mode: bool,
}

impl App {
    pub fn new(model: String, continue_session: bool, resume_session: Option<String>) -> Result<Self> {
        // Create message channel
        let (message_tx, message_rx) = mpsc::channel(100);

        // Initialize components
        let bash_executor = BashExecutor::new(message_tx.clone());
        let voice_recorder = VoiceRecorder::new(message_tx.clone());
        let session_manager = SessionManager::new(message_tx.clone())?;
        let command_manager = CommandManager::new();

        Ok(Self {
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
            input_history: Vec::new(),
            history_index: None,
            scroll_offset: 0,
            should_quit: false,
            token_usage: TokenUsage::default(),
            status_message: None,
            command_manager,
            autocomplete_suggestion: None,
            animation_tick: 0,
            attached_images: Vec::new(),
            whisper_dynamic_context: String::new(),
            auto_continue: true, // Ralph-style: enabled by default
            auto_continue_count: 0,
            max_auto_continues: 10, // Circuit breaker
            verbose_mode: false, // Collapsed tool view by default
        })
    }

    /// Main event loop
    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Register with session manager
        self.session_id = Some(self.session_manager.register("interactive").await?);

        // Add welcome message
        self.add_system_message(&format!(
            "Claude Terminal (model: {}) - Type /help for commands, Ctrl+Q to quit",
            self.model
        ));

        // Main loop
        loop {
            // Draw UI
            terminal.draw(|frame| {
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
                    autocomplete_suggestion: self.autocomplete_suggestion.as_deref(),
                    animation_tick: self.animation_tick,
                    total_messages: self.messages.len(),
                    verbose_mode: self.verbose_mode,
                };
                ui::draw(frame, &state);
            })?;

            // Update animation tick (wraps at 255)
            self.animation_tick = self.animation_tick.wrapping_add(1);

            // Handle events
            if self.handle_events().await? {
                break;
            }
        }

        // Cleanup
        self.cleanup(&mut terminal)?;
        Ok(())
    }

    async fn handle_events(&mut self) -> Result<bool> {
        // Check for app messages first
        while let Ok(msg) = self.message_rx.try_recv() {
            self.handle_app_message(msg).await?;
        }

        // Poll for input events
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    if self.handle_key_event(key).await? {
                        return Ok(true);
                    }
                }
                Event::Mouse(_) => {
                    // Mouse events disabled for text selection
                }
                _ => {}
            }
        }

        Ok(self.should_quit)
    }

    async fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        // Check for quit
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('q') {
            self.should_quit = true;
            return Ok(true);
        }

        // Check for interrupt
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            if self.claude_busy {
                if let Some(ref mut process) = self.claude_process {
                    process.abort().await;
                }
                self.add_system_message("Interrupted");
            }
            self.input.clear();
            self.cursor_position = 0;
            return Ok(false);
        }

        // Handle scroll keys (use contains for better macOS compatibility)
        match key.code {
            KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
                return Ok(false);
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                return Ok(false);
            }
            KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                return Ok(false);
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                return Ok(false);
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
                return Ok(false);
            }
            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                return Ok(false);
            }
            _ => {}
        }

        // Handle voice recording toggle (Alt+V for Voice)
        if key.code == KeyCode::Char('v') && key.modifiers.contains(KeyModifiers::ALT) {
            self.toggle_voice_recording().await?;
            return Ok(false);
        }

        // Regular input handling
        match key.code {
            // Option+Enter or Shift+Enter inserts newline
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT)
                || key.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                self.input.insert(self.cursor_position, '\n');
                self.cursor_position += 1;
            }
            // Regular Enter submits
            KeyCode::Enter => {
                if !self.input.is_empty() {
                    let input = std::mem::take(&mut self.input);
                    self.cursor_position = 0;
                    self.autocomplete_suggestion = None;

                    // Add to history
                    if self.input_history.last() != Some(&input) {
                        self.input_history.push(input.clone());
                    }
                    self.history_index = None;

                    // Reset scroll when sending
                    self.scroll_offset = 0;

                    // Process input
                    self.process_input(&input).await?;
                }
            }
            KeyCode::Tab => {
                if let Some(suggestion) = self.autocomplete_suggestion.take() {
                    self.input = suggestion;
                    self.cursor_position = self.input.len();
                }
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input.remove(self.cursor_position);
                    self.update_autocomplete();
                }
            }
            KeyCode::Delete => {
                if self.cursor_position < self.input.len() {
                    self.input.remove(self.cursor_position);
                    self.update_autocomplete();
                }
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_position < self.input.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_position = 0;
            }
            KeyCode::End => {
                self.cursor_position = self.input.len();
            }
            KeyCode::Up => {
                self.navigate_history(-1);
            }
            KeyCode::Down => {
                self.navigate_history(1);
            }
            // Ctrl key combinations
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_word_backward();
                self.update_autocomplete();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.drain(..self.cursor_position);
                self.cursor_position = 0;
                self.update_autocomplete();
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.truncate(self.cursor_position);
                self.update_autocomplete();
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor_position = 0;
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor_position = self.input.len();
            }
            // Regular character input
            KeyCode::Char(c) => {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
                self.update_autocomplete();
                // Check for image paths when space is typed (likely end of pasted path)
                if c == ' ' {
                    self.process_image_paths();
                }
            }
            _ => {}
        }

        Ok(false)
    }

    async fn process_input(&mut self, input: &str) -> Result<()> {
        if input.starts_with('!') {
            // Bash command
            let command = input[1..].trim();
            self.execute_bash(command).await?;
        } else if input.starts_with('/') {
            // Slash command
            self.handle_slash_command(input).await?;
        } else {
            // Regular message to Claude
            self.send_to_claude(input, true).await?;
        }
        Ok(())
    }

    async fn execute_bash(&mut self, command: &str) -> Result<()> {
        self.add_message(Role::Bash, ConversationContent::BashCommand {
            command: command.to_string(),
            output: String::new(),
            exit_code: -1,
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
                self.add_system_message("Conversation cleared");
            }
            "model" => {
                if !args.is_empty() {
                    self.model = args.to_string();
                    self.add_system_message(&format!("Model set to: {}", self.model));
                } else {
                    self.add_system_message(&format!("Current model: {}", self.model));
                }
            }
            "sessions" => {
                let sessions = self.session_manager.list_sessions().await?;
                if sessions.is_empty() {
                    self.add_system_message("No other active sessions");
                } else {
                    self.add_system_message("Active sessions:");
                    for s in sessions {
                        self.add_system_message(&format!("  {} ({}) - {}", s.id, s.cwd, s.task));
                    }
                }
            }
            "send" => {
                let send_parts: Vec<&str> = args.splitn(2, ' ').collect();
                if send_parts.len() == 2 {
                    self.session_manager
                        .send_message(send_parts[0], send_parts[1])
                        .await?;
                    self.add_system_message(&format!("Message sent to {}", send_parts[0]));
                } else {
                    self.add_system_message("Usage: /send <session-id> <message>");
                }
            }
            "broadcast" => {
                if !args.is_empty() {
                    self.session_manager.broadcast(args).await?;
                    self.add_system_message("Message broadcast to all sessions");
                } else {
                    self.add_system_message("Usage: /broadcast <message>");
                }
            }
            "inbox" => {
                let messages = self.session_manager.read_inbox().await?;
                if messages.is_empty() {
                    self.add_system_message("No new messages");
                } else {
                    for msg in messages {
                        self.add_system_message(&format!("[{}] {}", msg.from, msg.message));
                    }
                }
            }
            "auto" => {
                self.auto_continue = !self.auto_continue;
                let status = if self.auto_continue { "ON" } else { "OFF" };
                self.add_system_message(&format!("Auto-continue mode: {} (max {} iterations)", status, self.max_auto_continues));
            }
            "verbose" | "v" => {
                self.verbose_mode = !self.verbose_mode;
                let status = if self.verbose_mode { "ON (showing all tool details)" } else { "OFF (collapsed view)" };
                self.add_system_message(&format!("Verbose mode: {}", status));
            }
            "cwd" => {
                if let Some(cwd) = self.bash_executor.cwd().await {
                    self.add_system_message(&format!("Shell working directory: {}", cwd.display()));
                } else {
                    self.add_system_message("Shell not initialized yet. Run a command first.");
                }
            }
            "shell" => {
                self.add_system_message("Persistent Shell Info:");
                self.add_system_message("  Type: Persistent (state preserved between commands)");
                if let Some(cwd) = self.bash_executor.cwd().await {
                    self.add_system_message(&format!("  CWD:  {}", cwd.display()));
                } else {
                    self.add_system_message("  CWD:  Not initialized");
                }
                self.add_system_message("  Environment variables and aliases persist across commands.");
                self.add_system_message("  Use !export VAR=value to set env vars, !cd to change directory.");
            }
            "help" => {
                self.add_system_message("Commands:");
                self.add_system_message("  !<cmd>                Run bash command (persistent shell)");
                self.add_system_message("  /quit, /q             Exit");
                self.add_system_message("  /clear                Clear conversation");
                self.add_system_message("  /model <name>         Set model");
                self.add_system_message("  /auto                 Toggle auto-continue mode");
                self.add_system_message("  /verbose, /v          Toggle verbose tool output");
                self.add_system_message("  /cwd                  Show shell working directory");
                self.add_system_message("  /shell                Show persistent shell info");
                self.add_system_message("  /sessions             List sessions");
                self.add_system_message("  /send <id> <msg>      Send to session");
                self.add_system_message("  /broadcast <msg>      Broadcast to all");
                self.add_system_message("  /inbox                Read messages");
                self.add_system_message("");
                self.add_system_message("Shortcuts:");
                self.add_system_message("  Alt+V                 Toggle voice recording");
                self.add_system_message("  Ctrl+C                Interrupt/clear");
                self.add_system_message("  Ctrl+Q                Quit");
                self.add_system_message("  Shift+Up/Down         Scroll 1 line");
                self.add_system_message("  Ctrl+Up/Down          Scroll 5 lines");
                self.add_system_message("  PageUp/PageDown       Scroll 10 lines");
            }
            _ => {
                self.add_system_message(&format!("Unknown command: /{}", command));
            }
        }
        Ok(())
    }

    async fn send_to_claude(&mut self, message: &str, add_to_display: bool) -> Result<()> {
        // Process any remaining image paths in the message
        self.process_image_paths();

        // Expand image placeholders to actual paths for Claude
        let expanded_message = self.expand_image_placeholders(message);

        // Add user message to conversation (with placeholders for display)
        if add_to_display {
            self.add_message(Role::User, ConversationContent::Text(message.to_string()));
        }

        // Clear attached images after incorporating them
        self.attached_images.clear();

        // If Claude is busy, queue the message
        if self.claude_busy {
            self.message_queue.push(expanded_message);
            self.status_message = Some(format!("{} queued", self.message_queue.len()));
            return Ok(());
        }

        self.start_claude(&expanded_message).await
    }

    async fn start_claude(&mut self, message: &str) -> Result<()> {
        self.claude_busy = true;
        self.streaming_buffer.clear();

        let mut process = ClaudeProcess::new(
            &self.model,
            self.message_tx.clone(),
            self.continue_session,
            self.resume_session.take(),
        )?;

        // Build message with bash command context
        let full_message = self.build_message_with_context(message);
        process.send(&full_message).await?;
        self.claude_process = Some(process);

        Ok(())
    }

    /// Build a message that includes recent bash command context
    fn build_message_with_context(&self, message: &str) -> String {
        // Collect recent bash commands (last 5)
        let bash_commands: Vec<_> = self.messages.iter()
            .rev()
            .filter_map(|entry| {
                if let ConversationContent::BashCommand { command, output, exit_code } = &entry.content {
                    Some((command.clone(), output.clone(), *exit_code))
                } else {
                    None
                }
            })
            .take(5)
            .collect();

        if bash_commands.is_empty() {
            return message.to_string();
        }

        // Build context string
        let mut context = String::from("<context>\nRecent bash commands run by the user:\n");
        for (cmd, output, code) in bash_commands.iter().rev() {
            context.push_str(&format!("$ {}\n", cmd));
            if !output.is_empty() {
                // Truncate long output
                let truncated = if output.len() > 500 {
                    format!("{}...(truncated)", &output[..500])
                } else {
                    output.clone()
                };
                context.push_str(&truncated);
                if !truncated.ends_with('\n') {
                    context.push('\n');
                }
            }
            if *code != 0 {
                context.push_str(&format!("(exit code: {})\n", code));
            }
        }
        context.push_str("</context>\n\n");
        context.push_str(message);
        context
    }

    async fn handle_app_message(&mut self, msg: AppMessage) -> Result<()> {
        match msg {
            AppMessage::ClaudeEvent(event) => {
                self.handle_claude_event(event);
            }
            AppMessage::ClaudeFinished => {
                // Finalize streaming buffer
                let response_text = if !self.streaming_buffer.is_empty() {
                    let text = std::mem::take(&mut self.streaming_buffer);
                    self.add_message(Role::Assistant, ConversationContent::Text(text.clone()));
                    text
                } else {
                    String::new()
                };
                self.claude_busy = false;
                self.claude_process = None;
                self.status_message = None;

                // Process queued messages first (already displayed when queued, don't add again)
                if let Some(queued) = self.message_queue.pop() {
                    self.status_message = Some(format!("{} more queued", self.message_queue.len()));
                    self.auto_continue_count = 0; // Reset on user input
                    Box::pin(self.start_claude(&queued)).await?;
                } else if self.auto_continue && self.should_auto_continue(&response_text) {
                    // Ralph-style auto-continue
                    if self.auto_continue_count < self.max_auto_continues {
                        self.auto_continue_count += 1;
                        self.status_message = Some(format!("Auto-continue {}/{}", self.auto_continue_count, self.max_auto_continues));
                        Box::pin(self.start_claude("continue")).await?;
                    } else {
                        self.add_system_message("Auto-continue limit reached. Send a message to continue.");
                        self.auto_continue_count = 0;
                    }
                } else {
                    // Task appears complete
                    self.auto_continue_count = 0;
                }
            }
            AppMessage::ClaudeError(err) => {
                self.claude_busy = false;
                self.claude_process = None;
                self.streaming_buffer.clear();
                self.add_system_message(&format!("Error: {}", err));
            }
            AppMessage::BashOutput(output) => {
                // Update last bash entry with output
                if let Some(entry) = self.messages.last_mut() {
                    if let ConversationContent::BashCommand { output: out, .. } = &mut entry.content {
                        out.push_str(&output);
                    }
                }
            }
            AppMessage::BashFinished(exit_code) => {
                // Update exit code
                if let Some(entry) = self.messages.last_mut() {
                    if let ConversationContent::BashCommand { exit_code: code, .. } = &mut entry.content {
                        *code = exit_code;
                    }
                }
            }
            AppMessage::VoiceTranscription(text) => {
                self.input = text;
                self.cursor_position = self.input.len();
                self.input_mode = InputMode::Normal;
                self.status_message = None; // Clear transcribing status
            }
            AppMessage::VoiceError(err) => {
                self.add_message(Role::System, ConversationContent::Error(err));
                self.input_mode = InputMode::Normal;
                self.status_message = None; // Clear transcribing status
            }
            AppMessage::SessionMessage { from, message } => {
                self.add_system_message(&format!("[Message from {}] {}", from, message));
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
                // Finalize any streaming text first
                if !self.streaming_buffer.is_empty() {
                    let text = std::mem::take(&mut self.streaming_buffer);
                    self.update_whisper_context(&text);
                    self.add_message(Role::Assistant, ConversationContent::Text(text));
                }
                // Add tool name to context
                self.add_to_whisper_context(&name);
                self.add_message(Role::Tool, ConversationContent::ToolUse { name, input });
            }
            StreamEvent::ToolResult { name, result } => {
                self.add_message(Role::Tool, ConversationContent::ToolResult { name, result });
            }
            StreamEvent::Thinking(text) => {
                self.add_message(Role::Assistant, ConversationContent::Thinking(text));
            }
            StreamEvent::Usage { input_tokens, output_tokens, .. } => {
                self.token_usage.input_tokens = input_tokens;
                self.token_usage.output_tokens = output_tokens;
            }
            StreamEvent::Model(_) => {
                // Model info is handled elsewhere, ignore here
            }
        }
    }

    /// Ralph-style: Detect if Claude needs to continue working
    /// Returns true if the response indicates incomplete work
    fn should_auto_continue(&self, response: &str) -> bool {
        let response_lower = response.to_lowercase();

        // Completion signals - DON'T auto-continue
        let completion_patterns = [
            "let me know if",
            "feel free to",
            "is there anything else",
            "hope this helps",
            "task complete",
            "all done",
            "finished",
            "that's all",
            "that should",
            "this should",
        ];

        for pattern in completion_patterns {
            if response_lower.contains(pattern) {
                return false;
            }
        }

        // Continue signals - DO auto-continue
        let continue_patterns = [
            "let me continue",
            "i'll continue",
            "continuing",
            "i need to",
            "next, i",
            "now i'll",
            "now let me",
            "i'm going to",
            "working on",
            "in progress",
        ];

        for pattern in continue_patterns {
            if response_lower.contains(pattern) {
                return true;
            }
        }

        // Check if response ends mid-sentence (no period, question mark, or newline at end)
        let trimmed = response.trim();
        if !trimmed.is_empty() {
            let last_char = trimmed.chars().last().unwrap_or('.');
            if !matches!(last_char, '.' | '!' | '?' | ':' | '\n' | '`' | '"' | ')' | ']') {
                return true;
            }
        }

        // Check if there are pending tool results that might need follow-up
        // Look at recent messages for tool use without corresponding text
        let recent_messages: Vec<_> = self.messages.iter().rev().take(5).collect();
        let has_recent_tool = recent_messages.iter().any(|m| matches!(m.role, Role::Tool));
        let has_recent_text = recent_messages.iter().any(|m| {
            matches!(m.role, Role::Assistant) && matches!(m.content, ConversationContent::Text(_))
        });

        if has_recent_tool && !has_recent_text {
            return true;
        }

        false
    }

    /// Extract technical terms from text and add to Whisper context
    fn update_whisper_context(&mut self, text: &str) {
        // Extract backticked terms (code/identifiers)
        let mut in_backtick = false;
        let mut current_term = String::new();

        for ch in text.chars() {
            if ch == '`' {
                if in_backtick && !current_term.is_empty() {
                    self.add_to_whisper_context(&current_term);
                    current_term.clear();
                }
                in_backtick = !in_backtick;
            } else if in_backtick {
                current_term.push(ch);
            }
        }

        // Extract CamelCase and snake_case words (likely identifiers)
        for word in text.split_whitespace() {
            let clean: String = word.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
            if clean.len() >= 4 {
                // Check for CamelCase (has uppercase after lowercase)
                let has_camel = clean.chars().zip(clean.chars().skip(1))
                    .any(|(a, b)| a.is_lowercase() && b.is_uppercase());
                // Check for snake_case
                let has_snake = clean.contains('_') && clean.chars().any(|c| c.is_lowercase());

                if has_camel || has_snake {
                    self.add_to_whisper_context(&clean);
                }
            }
        }
    }

    /// Add a term to the Whisper dynamic context, keeping it compact
    fn add_to_whisper_context(&mut self, term: &str) {
        let term = term.trim();
        if term.is_empty() || term.len() < 3 {
            return;
        }

        // Don't add if already present
        if self.whisper_dynamic_context.contains(term) {
            return;
        }

        // Add with comma separator
        if !self.whisper_dynamic_context.is_empty() {
            self.whisper_dynamic_context.push_str(", ");
        }
        self.whisper_dynamic_context.push_str(term);

        // Keep context under 200 chars by removing oldest terms
        while self.whisper_dynamic_context.len() > 200 {
            if let Some(comma_pos) = self.whisper_dynamic_context.find(", ") {
                self.whisper_dynamic_context = self.whisper_dynamic_context[comma_pos + 2..].to_string();
            } else {
                break;
            }
        }
    }

    async fn toggle_voice_recording(&mut self) -> Result<()> {
        match self.input_mode {
            InputMode::Recording => {
                // Update UI immediately before stopping
                self.input_mode = InputMode::Normal;
                self.status_message = Some("Transcribing".to_string()); // Dots animated in status bar
                // Pass dynamic context for better transcription accuracy
                let ctx = if self.whisper_dynamic_context.is_empty() {
                    None
                } else {
                    Some(self.whisper_dynamic_context.clone())
                };
                self.voice_recorder.stop(ctx).await?;
            }
            InputMode::Normal => {
                // Update UI immediately before starting
                self.input_mode = InputMode::Recording;
                if let Err(e) = self.voice_recorder.start().await {
                    // Revert on error
                    self.input_mode = InputMode::Normal;
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    fn add_message(&mut self, role: Role, content: ConversationContent) {
        self.messages.push(ConversationEntry {
            role,
            content,
            timestamp: chrono::Utc::now(),
        });
    }

    fn add_system_message(&mut self, text: &str) {
        self.add_message(Role::System, ConversationContent::Text(text.to_string()));
    }

    fn navigate_history(&mut self, direction: i32) {
        if self.input_history.is_empty() {
            return;
        }

        let new_index = match self.history_index {
            Some(i) => {
                let new = i as i32 + direction;
                if new < 0 {
                    None
                } else if new >= self.input_history.len() as i32 {
                    Some(self.input_history.len() - 1)
                } else {
                    Some(new as usize)
                }
            }
            None => {
                if direction < 0 {
                    Some(self.input_history.len() - 1)
                } else {
                    None
                }
            }
        };

        self.history_index = new_index;
        self.input = match new_index {
            Some(i) => self.input_history[i].clone(),
            None => String::new(),
        };
        self.cursor_position = self.input.len();
    }

    fn delete_word_backward(&mut self) {
        let (new_input, new_pos) = crate::input_utils::delete_word_backward(&self.input, self.cursor_position);
        self.input = new_input;
        self.cursor_position = new_pos;
    }

    /// Expand image placeholders back to actual paths for sending to Claude
    fn expand_image_placeholders(&self, message: &str) -> String {
        let mut result = message.to_string();
        for (i, path) in self.attached_images.iter().enumerate() {
            let placeholder = format!("[Image #{}]", i + 1);
            result = result.replace(&placeholder, path);
        }
        result
    }

    /// Check if a path looks like an image file
    fn is_image_path(path: &str) -> bool {
        let extensions = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg"];
        let lower = path.to_lowercase();
        extensions.iter().any(|ext| lower.ends_with(ext))
    }

    /// Process input to detect and replace image paths with placeholders
    fn process_image_paths(&mut self) {
        // Look for file paths in the input (starting with / or ~)
        let mut new_input = String::new();
        let mut last_end = 0;
        let input = self.input.clone();

        // Simple regex-like matching for paths
        let mut i = 0;
        let chars: Vec<char> = input.chars().collect();

        while i < chars.len() {
            // Check for path start (/ or ~)
            if chars[i] == '/' || (chars[i] == '~' && (i == 0 || chars[i - 1].is_whitespace())) {
                let start = i;
                // Find end of path (whitespace or end of string)
                // Handle escaped spaces (\ ) as part of the path
                while i < chars.len() {
                    if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == ' ' {
                        // Escaped space - skip both characters
                        i += 2;
                    } else if chars[i].is_whitespace() {
                        // Unescaped whitespace - end of path
                        break;
                    } else {
                        i += 1;
                    }
                }
                let path: String = chars[start..i].iter().collect();

                // Unescape the path (remove backslashes before spaces)
                let unescaped_path = path.replace("\\ ", " ");

                // Expand ~ to home directory
                let expanded_path = if unescaped_path.starts_with('~') {
                    if let Some(home) = dirs::home_dir() {
                        unescaped_path.replacen('~', &home.display().to_string(), 1)
                    } else {
                        unescaped_path.clone()
                    }
                } else {
                    unescaped_path.clone()
                };

                // Check if it's an image and exists
                if Self::is_image_path(&expanded_path) && Path::new(&expanded_path).exists() {
                    // Add text before this path
                    let before: String = chars[last_end..start].iter().collect();
                    new_input.push_str(&before);

                    // Add placeholder and store path
                    self.attached_images.push(expanded_path);
                    let placeholder = format!("[Image #{}]", self.attached_images.len());
                    new_input.push_str(&placeholder);

                    last_end = i;
                }
            } else {
                i += 1;
            }
        }

        // If we found any images, update the input
        if !self.attached_images.is_empty() && last_end > 0 {
            let remainder: String = chars[last_end..].iter().collect();
            new_input.push_str(&remainder);

            // Adjust cursor position
            let old_len = self.input.len();
            self.input = new_input;
            if self.cursor_position > self.input.len() {
                self.cursor_position = self.input.len();
            }
        }
    }

    fn update_autocomplete(&mut self) {
        if self.input.starts_with('/') && !self.input.contains(' ') {
            let completions = self.command_manager.get_completions(&self.input);
            if let Some(cmd) = completions.first() {
                let suggestion = format!("/{}", cmd.name);
                if suggestion != self.input {
                    self.autocomplete_suggestion = Some(suggestion);
                    return;
                }
            }
        }
        self.autocomplete_suggestion = None;
    }

    fn cleanup(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        // Remove session file
        if let Some(ref session_id) = self.session_id {
            let _ = std::fs::remove_file(format!(
                "{}/.claude-sessions/{}.json",
                dirs::home_dir().unwrap().display(),
                session_id
            ));
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        Ok(())
    }
}
