# claude-terminal

A fast, responsive terminal interface for Claude Code built in Rust.

## Features

- **Fast UI**: Built with Rust + ratatui for zero input lag
- **Always-available input**: Type while Claude is thinking, queue messages
- **Bash integration**: Run `!command` and Claude sees the output
- **Voice input**: Press `*` to record, `*` again to transcribe with Whisper
- **Session integration**: Compatible with claude-sessions for parallel work
- **Max subscription**: Uses your Claude Max plan via CLI wrapper

## Installation

```bash
cargo install --path .
```

Or build manually:

```bash
cargo build --release
./target/release/claude-terminal
```

## Usage

```bash
# Start with default model (sonnet)
claude-terminal

# Specify a model
claude-terminal --model opus

# Continue previous session
claude-terminal --continue

# Resume specific session
claude-terminal --resume <session-id>

# Work in a specific directory
claude-terminal -d /path/to/project
```

## Commands

| Command | Description |
|---------|-------------|
| `!<cmd>` | Run bash command (e.g., `!ls -la`) |
| `/quit` | Exit |
| `/clear` | Clear conversation |
| `/model <name>` | Switch model (sonnet, opus, haiku) |
| `/sessions` | List active Claude sessions |
| `/send <id> <msg>` | Send message to another session |
| `/broadcast <msg>` | Broadcast to all sessions |
| `/inbox` | Read incoming messages |
| `/help` | Show help |

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `*` | Toggle voice recording |
| `Ctrl+C` | Interrupt Claude / Clear input |
| `Ctrl+Q` | Quit |
| `↑/↓` | Navigate input history |
| `PageUp/PageDown` | Scroll conversation |

## Voice Input

Voice recording uses the OpenAI Whisper API. Set your API key:

```bash
export OPENAI_API_KEY=sk-...
```

Press `*` to start recording, `*` again to stop. The transcription appears in the input field for review before sending.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `OPENAI_API_KEY` | Required for voice transcription |
| `CLAUDE_TERMINAL_MODEL` | Default model (default: sonnet) |

## Claude Sessions Integration

Compatible with the claude-sessions system for parallel Claude instances:

- Session is automatically registered on startup
- Use `/sessions` to see other active Claude sessions
- Use `/send` and `/broadcast` to communicate between sessions
- Session is deregistered on exit

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        claude-terminal                          │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   TUI Layer  │  │ Voice Layer  │  │  Session Manager     │  │
│  │   (ratatui)  │  │  (Whisper)   │  │  (claude-sessions)   │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │
│         │                 │                      │              │
│  ┌──────▼─────────────────▼──────────────────────▼───────────┐  │
│  │                    Message Router                          │  │
│  └──────────────────────────┬────────────────────────────────┘  │
│                             │                                   │
│  ┌──────────────────────────▼────────────────────────────────┐  │
│  │                 Claude Process Manager                     │  │
│  │  - Spawns: claude --print --output-format stream-json     │  │
│  │  - Parses JSON events, handles abort/interrupt            │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Building from Source

Requirements:
- Rust 1.70+
- macOS, Linux, or Windows

```bash
git clone https://github.com/youruser/claude-terminal
cd claude-terminal
cargo build --release
```

## License

MIT
