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
    Migration {
        version: "023_video_pessoas",
        sql: include_str!("../../migrations/023_video_pessoas.sql"),
    },
    Migration {
        version: "024_video_rastreio_classes",
        sql: include_str!("../../migrations/024_video_rastreio_classes.sql"),
    },
    Migration {
        version: "025_video_realce",
        sql: include_str!("../../migrations/025_video_realce.sql"),
    },
    Migration {
        version: "026_video_media_repetido",
        sql: include_str!("../../migrations/026_video_media_repetido.sql"),
    },
    Migration {
        version: "027_edicao",
        sql: include_str!("../../migrations/027_edicao.sql"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migracao_027_sobre_banco_com_video() {
        // Banco de antes da 027 com um vídeo: a linha fica e as colunas novas vêm nulas.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version TEXT PRIMARY KEY, applied_at TEXT NOT NULL);",
        )
        .unwrap();
        for m in MIGRATIONS.iter().filter(|m| m.version < "027") {
            conn.execute_batch(m.sql).unwrap();
            conn.execute("INSERT INTO schema_migrations VALUES (?1, 'x')", [m.version]).unwrap();
        }
        let occ = "00000000-0000-4000-8000-000000000001";
        conn.execute("INSERT INTO occurrences (id, created_at, updated_at) VALUES (?1, 'x', 'x')", [occ])
            .unwrap();
        conn.execute(
            "INSERT INTO video_media (id, occurrence_id, relative_path, filename, sha256, created_at, updated_at)
             VALUES ('v1', ?1, 'a.mp4', 'a.mp4', 'h', 'x', 'x')",
            [occ],
        )
        .unwrap();
        run_migrations(&mut conn).unwrap();
        let (sha, nativo, obs, desvio, gravador): (String, Option<i64>, Option<String>, Option<f64>, Option<String>) = conn
            .query_row(
                "SELECT sha256, recebido_nativo, recebido_obs, relogio_desvio_s, gravador FROM video_media WHERE id = 'v1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!((sha.as_str(), nativo, obs, desvio, gravador), ("h", None, None, None, None));
        // Rodar de novo não reaplica nada.
        run_migrations(&mut conn).unwrap();
    }
}
