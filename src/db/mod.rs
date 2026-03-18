//! Database module for persistent session storage
//!
//! Provides SQLite-based storage for sessions, messages, and file history.

mod models;
mod schema;

pub use models::*;

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Database handle with connection pooling
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Database {
    /// Open or create the database at the given path
    pub fn open(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&path)?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
        };

        // Run migrations
        db.migrate()?;

        Ok(db)
    }

    /// Open the default database in ~/.claude-terminal/
    pub fn open_default() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let db_dir = home.join(".claude-terminal");
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("sessions.db");
        Self::open(db_path)
    }

    /// Run database migrations
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        schema::migrate(&conn)
    }

    /// Get the database path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    // ========== Session Operations ==========

    /// Create a new session
    pub fn create_session(&self, session: &DbSession) -> Result<i64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        conn.execute(
            "INSERT INTO sessions (id, pid, cwd, task, app, tmux_window, parent_id, model, token_count, cost)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            (
                &session.id,
                session.pid,
                &session.cwd,
                &session.task,
                &session.app,
                &session.tmux_window,
                &session.parent_id,
                &session.model,
                session.token_count,
                session.cost,
            ),
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &str) -> Result<Option<DbSession>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, pid, cwd, task, app, tmux_window, parent_id, model, token_count, cost, created_at, updated_at
             FROM sessions WHERE id = ?1",
        )?;

        let result = stmt.query_row([id], |row| {
            Ok(DbSession {
                id: row.get(0)?,
                pid: row.get(1)?,
                cwd: row.get(2)?,
                task: row.get(3)?,
                app: row.get(4)?,
                tmux_window: row.get(5)?,
                parent_id: row.get(6)?,
                model: row.get(7)?,
                token_count: row.get(8)?,
                cost: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        });

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all active sessions
    pub fn list_sessions(&self) -> Result<Vec<DbSession>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, pid, cwd, task, app, tmux_window, parent_id, model, token_count, cost, created_at, updated_at
             FROM sessions WHERE parent_id IS NULL ORDER BY created_at DESC",
        )?;

        let sessions = stmt.query_map([], |row| {
            Ok(DbSession {
                id: row.get(0)?,
                pid: row.get(1)?,
                cwd: row.get(2)?,
                task: row.get(3)?,
                app: row.get(4)?,
                tmux_window: row.get(5)?,
                parent_id: row.get(6)?,
                model: row.get(7)?,
                token_count: row.get(8)?,
                cost: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;

        sessions.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Update session token count and cost
    pub fn update_session_usage(&self, id: &str, token_count: i64, cost: f64) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        conn.execute(
            "UPDATE sessions SET token_count = ?1, cost = ?2, updated_at = strftime('%s', 'now') * 1000 WHERE id = ?3",
            (token_count, cost, id),
        )?;
        Ok(())
    }

    /// Update session task description
    pub fn update_session_task(&self, id: &str, task: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        conn.execute(
            "UPDATE sessions SET task = ?1, updated_at = strftime('%s', 'now') * 1000 WHERE id = ?2",
            (task, id),
        )?;
        Ok(())
    }

    /// Delete a session and all related data (cascades)
    pub fn delete_session(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    // ========== Message Operations ==========

    /// Add a message to a session
    pub fn add_message(&self, message: &DbMessage) -> Result<i64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        conn.execute(
            "INSERT INTO messages (session_id, role, content, model, tool_calls)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &message.session_id,
                &message.role,
                &message.content,
                &message.model,
                &message.tool_calls,
            ),
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Get messages for a session
    pub fn get_messages(&self, session_id: &str) -> Result<Vec<DbMessage>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, model, tool_calls, created_at
             FROM messages WHERE session_id = ?1 ORDER BY created_at ASC",
        )?;

        let messages = stmt.query_map([session_id], |row| {
            Ok(DbMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                model: row.get(4)?,
                tool_calls: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        messages.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get the last N messages for a session
    pub fn get_recent_messages(&self, session_id: &str, limit: i64) -> Result<Vec<DbMessage>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, model, tool_calls, created_at
             FROM messages WHERE session_id = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;

        let messages = stmt.query_map([session_id, &limit.to_string()], |row| {
            Ok(DbMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                model: row.get(4)?,
                tool_calls: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut result: Vec<_> = messages.collect::<std::result::Result<Vec<_>, _>>()?;
        result.reverse(); // Return in chronological order
        Ok(result)
    }

    // ========== File History Operations ==========

    /// Record a file modification
    pub fn add_file_history(&self, history: &DbFileHistory) -> Result<i64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        conn.execute(
            "INSERT INTO file_history (session_id, path, content_before, content_after, message_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &history.session_id,
                &history.path,
                &history.content_before,
                &history.content_after,
                &history.message_id,
            ),
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Get file history for a session
    pub fn get_file_history(&self, session_id: &str) -> Result<Vec<DbFileHistory>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, path, content_before, content_after, message_id, created_at
             FROM file_history WHERE session_id = ?1 ORDER BY created_at DESC",
        )?;

        let history = stmt.query_map([session_id], |row| {
            Ok(DbFileHistory {
                id: row.get(0)?,
                session_id: row.get(1)?,
                path: row.get(2)?,
                content_before: row.get(3)?,
                content_after: row.get(4)?,
                message_id: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        history.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get file history for a specific file in a session
    pub fn get_file_history_by_path(&self, session_id: &str, path: &str) -> Result<Vec<DbFileHistory>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, path, content_before, content_after, message_id, created_at
             FROM file_history WHERE session_id = ?1 AND path = ?2 ORDER BY created_at DESC",
        )?;

        let history = stmt.query_map([session_id, path], |row| {
            Ok(DbFileHistory {
                id: row.get(0)?,
                session_id: row.get(1)?,
                path: row.get(2)?,
                content_before: row.get(3)?,
                content_after: row.get(4)?,
                message_id: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        history.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    // ========== Session Message Queue Operations ==========

    /// Add a message to the session inbox
    pub fn add_inbox_message(&self, target_session_id: &str, from_session_id: &str, message: &str) -> Result<i64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        conn.execute(
            "INSERT INTO session_inbox (target_session_id, from_session_id, message)
             VALUES (?1, ?2, ?3)",
            (target_session_id, from_session_id, message),
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Get and clear inbox messages for a session
    pub fn pop_inbox_messages(&self, session_id: &str) -> Result<Vec<DbInboxMessage>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT id, target_session_id, from_session_id, message, created_at
             FROM session_inbox WHERE target_session_id = ?1 ORDER BY created_at ASC",
        )?;

        let messages: Vec<DbInboxMessage> = stmt
            .query_map([session_id], |row| {
                Ok(DbInboxMessage {
                    id: row.get(0)?,
                    target_session_id: row.get(1)?,
                    from_session_id: row.get(2)?,
                    message: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Clear the inbox
        conn.execute("DELETE FROM session_inbox WHERE target_session_id = ?1", [session_id])?;

        Ok(messages)
    }

    /// Check if there are pending inbox messages
    pub fn has_inbox_messages(&self, session_id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_inbox WHERE target_session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            conn: Arc::clone(&self.conn),
            path: self.path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_db() -> Database {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        Database::open(path).unwrap()
    }

    #[test]
    fn test_create_and_get_session() {
        let db = test_db();

        let session = DbSession {
            id: "test-session-1".to_string(),
            pid: 12345,
            cwd: "/home/user".to_string(),
            task: "Test task".to_string(),
            app: Some("claude-terminal".to_string()),
            tmux_window: None,
            parent_id: None,
            model: Some("claude-3-opus".to_string()),
            token_count: 0,
            cost: 0.0,
            created_at: 0,
            updated_at: 0,
        };

        db.create_session(&session).unwrap();

        let retrieved = db.get_session("test-session-1").unwrap().unwrap();
        assert_eq!(retrieved.id, "test-session-1");
        assert_eq!(retrieved.pid, 12345);
        assert_eq!(retrieved.task, "Test task");
    }

    #[test]
    fn test_add_and_get_messages() {
        let db = test_db();

        let session = DbSession {
            id: "msg-test-session".to_string(),
            pid: 12345,
            cwd: "/home/user".to_string(),
            task: "Test".to_string(),
            app: None,
            tmux_window: None,
            parent_id: None,
            model: None,
            token_count: 0,
            cost: 0.0,
            created_at: 0,
            updated_at: 0,
        };
        db.create_session(&session).unwrap();

        let msg1 = DbMessage {
            id: 0,
            session_id: "msg-test-session".to_string(),
            role: "user".to_string(),
            content: "Hello".to_string(),
            model: None,
            tool_calls: None,
            created_at: 0,
        };
        db.add_message(&msg1).unwrap();

        let msg2 = DbMessage {
            id: 0,
            session_id: "msg-test-session".to_string(),
            role: "assistant".to_string(),
            content: "Hi there!".to_string(),
            model: Some("claude-3-opus".to_string()),
            tool_calls: None,
            created_at: 0,
        };
        db.add_message(&msg2).unwrap();

        let messages = db.get_messages("msg-test-session").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }

    #[test]
    fn test_file_history() {
        let db = test_db();

        let session = DbSession {
            id: "file-test-session".to_string(),
            pid: 12345,
            cwd: "/home/user".to_string(),
            task: "Test".to_string(),
            app: None,
            tmux_window: None,
            parent_id: None,
            model: None,
            token_count: 0,
            cost: 0.0,
            created_at: 0,
            updated_at: 0,
        };
        db.create_session(&session).unwrap();

        let history = DbFileHistory {
            id: 0,
            session_id: "file-test-session".to_string(),
            path: "/home/user/test.rs".to_string(),
            content_before: Some("old content".to_string()),
            content_after: Some("new content".to_string()),
            message_id: None,
            created_at: 0,
        };
        db.add_file_history(&history).unwrap();

        let retrieved = db.get_file_history("file-test-session").unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].path, "/home/user/test.rs");
    }

    #[test]
    fn test_inbox_messages() {
        let db = test_db();

        // Create two sessions
        let session1 = DbSession {
            id: "session-1".to_string(),
            pid: 12345,
            cwd: "/home/user".to_string(),
            task: "Task 1".to_string(),
            app: None,
            tmux_window: None,
            parent_id: None,
            model: None,
            token_count: 0,
            cost: 0.0,
            created_at: 0,
            updated_at: 0,
        };
        let session2 = DbSession {
            id: "session-2".to_string(),
            pid: 12346,
            cwd: "/home/user".to_string(),
            task: "Task 2".to_string(),
            app: None,
            tmux_window: None,
            parent_id: None,
            model: None,
            token_count: 0,
            cost: 0.0,
            created_at: 0,
            updated_at: 0,
        };
        db.create_session(&session1).unwrap();
        db.create_session(&session2).unwrap();

        // Send message from session 1 to session 2
        db.add_inbox_message("session-2", "session-1", "Hello from session 1").unwrap();

        // Check inbox
        assert!(db.has_inbox_messages("session-2").unwrap());
        assert!(!db.has_inbox_messages("session-1").unwrap());

        // Pop messages
        let messages = db.pop_inbox_messages("session-2").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message, "Hello from session 1");

        // Inbox should be empty now
        assert!(!db.has_inbox_messages("session-2").unwrap());
    }

    #[test]
    fn test_cascade_delete() {
        let db = test_db();

        let session = DbSession {
            id: "cascade-test".to_string(),
            pid: 12345,
            cwd: "/home/user".to_string(),
            task: "Test".to_string(),
            app: None,
            tmux_window: None,
            parent_id: None,
            model: None,
            token_count: 0,
            cost: 0.0,
            created_at: 0,
            updated_at: 0,
        };
        db.create_session(&session).unwrap();

        let msg = DbMessage {
            id: 0,
            session_id: "cascade-test".to_string(),
            role: "user".to_string(),
            content: "Test message".to_string(),
            model: None,
            tool_calls: None,
            created_at: 0,
        };
        db.add_message(&msg).unwrap();

        // Delete session - should cascade to messages
        db.delete_session("cascade-test").unwrap();

        // Session should be gone
        assert!(db.get_session("cascade-test").unwrap().is_none());

        // Messages should also be gone
        assert!(db.get_messages("cascade-test").unwrap().is_empty());
    }
}
