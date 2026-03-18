//! Command management for built-in and custom slash commands

use std::path::PathBuf;

/// A slash command definition
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Command {
    /// Command name (without the leading /)
    pub name: String,
    /// Short description
    pub description: String,
    /// Whether this is a custom command from ~/.claude/commands
    pub is_custom: bool,
}

/// Manages available commands
pub struct CommandManager {
    commands: Vec<Command>,
}

impl CommandManager {
    /// Create a new CommandManager and load all commands
    pub fn new() -> Self {
        let mut manager = Self {
            commands: Vec::new(),
        };
        manager.load_builtin_commands();
        manager.load_custom_commands();
        manager
    }

    /// Load built-in commands
    fn load_builtin_commands(&mut self) {
        let builtins = [
            ("auto", "Toggle auto-continue mode (ralph-style)"),
            ("broadcast", "Broadcast to all sessions"),
            ("clear", "Clear conversation"),
            ("cwd", "Show shell working directory"),
            ("help", "Show help"),
            ("inbox", "Read incoming messages"),
            ("model", "Set or show current model"),
            ("q", "Exit the application"),
            ("quit", "Exit the application"),
            ("send", "Send message to another session"),
            ("sessions", "List active sessions"),
            ("shell", "Show persistent shell info"),
            ("v", "Toggle verbose tool output"),
            ("verbose", "Toggle verbose tool output"),
        ];

        for (name, desc) in builtins {
            self.commands.push(Command {
                name: name.to_string(),
                description: desc.to_string(),
                is_custom: false,
            });
        }
    }

    /// Load custom commands from ~/.claude/commands
    fn load_custom_commands(&mut self) {
        let commands_dir = dirs::home_dir()
            .map(|h| h.join(".claude").join("commands"))
            .unwrap_or_else(|| PathBuf::from(".claude/commands"));

        if let Ok(entries) = std::fs::read_dir(&commands_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        // Try to read description from frontmatter
                        let description = Self::read_command_description(&path)
                            .unwrap_or_else(|| "Custom command".to_string());

                        self.commands.push(Command {
                            name: name.to_string(),
                            description,
                            is_custom: true,
                        });
                    }
                }
            }
        }

        // Sort commands alphabetically
        self.commands.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Read the description from a command's YAML frontmatter
    fn read_command_description(path: &PathBuf) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;
        let lines: Vec<&str> = content.lines().collect();

        // Check for YAML frontmatter
        if lines.first()? != &"---" {
            return None;
        }

        for line in lines.iter().skip(1) {
            if *line == "---" {
                break;
            }
            if let Some(desc) = line.strip_prefix("description:") {
                return Some(desc.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }

        None
    }

    /// Get commands that match a prefix
    pub fn get_completions(&self, prefix: &str) -> Vec<&Command> {
        let prefix = prefix.strip_prefix('/').unwrap_or(prefix);
        self.commands
            .iter()
            .filter(|cmd| cmd.name.starts_with(prefix))
            .collect()
    }

    /// Check if a command name exists
    #[allow(dead_code)]
    pub fn is_command(&self, name: &str) -> bool {
        let name = name.strip_prefix('/').unwrap_or(name);
        self.commands.iter().any(|cmd| cmd.name == name)
    }

    /// Get all commands
    #[allow(dead_code)]
    pub fn all_commands(&self) -> &[Command] {
        &self.commands
    }
}

impl Default for CommandManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_commands_loaded() {
        let manager = CommandManager::new();
        let all = manager.all_commands();
        assert!(!all.is_empty(), "Should have loaded some commands");
    }

    #[test]
    fn test_get_completions_prefix() {
        let manager = CommandManager::new();

        // Test with /q prefix
        let completions = manager.get_completions("/q");
        assert!(!completions.is_empty(), "Should find commands starting with 'q'");
        assert!(completions.iter().any(|c| c.name == "quit" || c.name == "q"));
    }

    #[test]
    fn test_get_completions_full_match() {
        let manager = CommandManager::new();

        // Test with full command name
        let completions = manager.get_completions("/quit");
        assert!(completions.iter().any(|c| c.name == "quit"));
    }

    #[test]
    fn test_get_completions_no_match() {
        let manager = CommandManager::new();

        // Test with prefix that matches nothing
        let completions = manager.get_completions("/xyz");
        assert!(completions.is_empty(), "Should not find any commands");
    }

    #[test]
    fn test_is_command() {
        let manager = CommandManager::new();

        assert!(manager.is_command("quit"));
        assert!(manager.is_command("/quit"));
        assert!(manager.is_command("help"));
        assert!(!manager.is_command("nonexistent"));
    }

    #[test]
    fn test_custom_commands_loaded() {
        let manager = CommandManager::new();

        // Check if custom commands were loaded (depends on ~/.claude/commands existing)
        // This test may pass or fail depending on the environment
        let all = manager.all_commands();

        // At minimum, built-in commands should exist
        assert!(all.iter().any(|c| c.name == "help"));
        assert!(all.iter().any(|c| c.name == "quit"));
    }

    #[test]
    fn test_commands_sorted_alphabetically() {
        let manager = CommandManager::new();
        let all = manager.all_commands();

        let names: Vec<_> = all.iter().map(|c| &c.name).collect();
        let mut sorted = names.clone();
        sorted.sort();

        assert_eq!(names, sorted, "Commands should be sorted alphabetically");
    }
}
