//! Migrações embutidas no binário (`include_str!`), versionadas em `schema_migrations`.
//! Nova migração: criar `migrations/NNN_label.sql` (idempotente, IF NOT EXISTS)
//! e registrar em `MIGRATIONS`.

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
    Migration {
        version: "018_video_clock",
        sql: include_str!("../../migrations/018_video_clock.sql"),
    },
    Migration {
        version: "019_video_derived",
        sql: include_str!("../../migrations/019_video_derived.sql"),
    },
    Migration {
        version: "020_transcript_ai",
        sql: include_str!("../../migrations/020_transcript_ai.sql"),
    },
    Migration {
        version: "021_audio_diarization",
        sql: include_str!("../../migrations/021_audio_diarization.sql"),
    },
    Migration {
        version: "022_occurrence_titulo",
        sql: include_str!("../../migrations/022_occurrence_titulo.sql"),
    },
];

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    // A 001 também cria schema_migrations, mas é preciso lê-la antes da primeira rodar.
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
