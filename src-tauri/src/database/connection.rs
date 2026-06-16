//! SQLite connection setup.
//!
//! Each workspace owns its own SQLite file. We open one connection per
//! command — SQLite handles concurrent connections at OS level and the
//! workspaces are single-process in Spike A, so a connection pool is overkill.

use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;

pub fn open_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    apply_pragmas(&conn)?;
    Ok(conn)
}

/// Pragmas appropriate for desktop usage: WAL for durability under crashes,
/// foreign keys on, normal synchronous (good balance for desktop), 1s busy
/// timeout so transient locks don't surface as errors.
fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 1000)?;
    Ok(())
}
