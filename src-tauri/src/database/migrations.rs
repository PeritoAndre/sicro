//! Schema migrations.
//!
//! Migrations are bundled at compile time via `include_str!`, so they ship
//! inside the executable. Versions are tracked in `schema_migrations`.
//!
//! Adding a new migration:
//!   1. Drop a file in `src-tauri/migrations/NNN_label.sql` (must be IF NOT EXISTS-safe).
//!   2. Register it in the `MIGRATIONS` array below.
//!   3. Bump `current_version()` if you also want to surface it.

use chrono::Utc;
use rusqlite::Connection;

use crate::error::Result;

struct Migration {
    version: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: "001_initial",
        sql: include_str!("../../migrations/001_initial.sql"),
    },
    Migration {
        version: "002_laudos",
        sql: include_str!("../../migrations/002_laudos.sql"),
    },
    Migration {
        version: "003_exports",
        sql: include_str!("../../migrations/003_exports.sql"),
    },
    Migration {
        version: "004_imports",
        sql: include_str!("../../migrations/004_imports.sql"),
    },
    Migration {
        version: "005_dossie",
        sql: include_str!("../../migrations/005_dossie.sql"),
    },
    Migration {
        version: "006_croquis",
        sql: include_str!("../../migrations/006_croquis.sql"),
    },
    Migration {
        version: "007_video",
        sql: include_str!("../../migrations/007_video.sql"),
    },
    Migration {
        version: "008_evidence_links",
        sql: include_str!("../../migrations/008_evidence_links.sql"),
    },
    Migration {
        version: "009_image_analyses",
        sql: include_str!("../../migrations/009_image_analyses.sql"),
    },
    Migration {
        version: "010_video_speed",
        sql: include_str!("../../migrations/010_video_speed.sql"),
    },
    Migration {
        version: "011_video_distance",
        sql: include_str!("../../migrations/011_video_distance.sql"),
    },
    Migration {
        version: "012_audio",
        sql: include_str!("../../migrations/012_audio.sql"),
    },
    Migration {
        version: "013_audio_markers",
        sql: include_str!("../../migrations/013_audio_markers.sql"),
    },
    Migration {
        version: "014_audio_enhancements",
        sql: include_str!("../../migrations/014_audio_enhancements.sql"),
    },
    Migration {
        version: "015_audio_transcript",
        sql: include_str!("../../migrations/015_audio_transcript.sql"),
    },
    Migration {
        version: "016_documentoscopia",
        sql: include_str!("../../migrations/016_documentoscopia.sql"),
    },
    Migration {
        version: "017_croqui_kind",
        sql: include_str!("../../migrations/017_croqui_kind.sql"),
    },
];

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    // schema_migrations is also created by 001 itself, but we need to read
    // from it before the first migration runs, so create it idempotently here.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;

    for migration in MIGRATIONS {
        let already_applied: bool = conn
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if already_applied {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            [migration.version, Utc::now().to_rfc3339().as_str()],
        )?;
        tx.commit()?;
        tracing::info!("applied migration {}", migration.version);
    }

    Ok(())
}
