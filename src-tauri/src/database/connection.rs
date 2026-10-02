//! Conexão SQLite: um arquivo por workspace, uma conexão por comando
//! (processo único — pool seria exagero).

use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;

pub fn open_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    apply_pragmas(&conn)?;
    Ok(conn)
}

/// WAL (durabilidade em queda), foreign keys on, synchronous NORMAL e
/// busy_timeout de 1 s para lock transitório não virar erro.
fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 1000)?;
    Ok(())
}
