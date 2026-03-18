//! Database schema and migrations

use anyhow::Result;
use rusqlite::Connection;

/// Current schema version
const SCHEMA_VERSION: i64 = 1;

/// Run all pending migrations
pub fn migrate(conn: &Connection) -> Result<()> {
    // Create migrations table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        )",
        [],
    )?;

    // Get current version
    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Apply migrations
    if current_version < 1 {
        migrate_v1(conn)?;
    }

    Ok(())
}

/// Migration v1: Initial schema
fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Sessions table
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            pid INTEGER NOT NULL,
            cwd TEXT NOT NULL,
            task TEXT NOT NULL,
            app TEXT,
            tmux_window TEXT,
            parent_id TEXT REFERENCES sessions(id) ON DELETE CASCADE,
            model TEXT,
            token_count INTEGER DEFAULT 0,
            cost REAL DEFAULT 0.0,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000),
            updated_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );

        CREATE INDEX IF NOT EXISTS idx_sessions_parent_id ON sessions(parent_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_created_at ON sessions(created_at);

        -- Messages table
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            model TEXT,
            tool_calls TEXT,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );

        CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id);
        CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);

        -- File history table
        CREATE TABLE IF NOT EXISTS file_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            path TEXT NOT NULL,
            content_before TEXT,
            content_after TEXT,
            message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );

        CREATE INDEX IF NOT EXISTS idx_file_history_session_id ON file_history(session_id);
        CREATE INDEX IF NOT EXISTS idx_file_history_path ON file_history(session_id, path);

        -- Session inbox for cross-session messaging
        CREATE TABLE IF NOT EXISTS session_inbox (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            target_session_id TEXT NOT NULL,
            from_session_id TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at INTEGER DEFAULT (strftime('%s', 'now') * 1000)
        );

        CREATE INDEX IF NOT EXISTS idx_session_inbox_target ON session_inbox(target_session_id);

        -- Trigger to auto-update updated_at on sessions
        CREATE TRIGGER IF NOT EXISTS sessions_updated_at
        AFTER UPDATE ON sessions
        FOR EACH ROW
        BEGIN
            UPDATE sessions SET updated_at = strftime('%s', 'now') * 1000 WHERE id = NEW.id;
        END;

        -- Record migration
        INSERT INTO schema_migrations (version) VALUES (1);
        "#,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_fresh_database() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"file_history".to_string()));
        assert!(tables.contains(&"session_inbox".to_string()));
        assert!(tables.contains(&"schema_migrations".to_string()));
    }

    #[test]
    fn test_migrate_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations twice
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();

        // Should still work
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(version, SCHEMA_VERSION);
    }
}
