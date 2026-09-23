//! Catalogue schema and migrations.
//!
//! Every entry in [`MIGRATIONS`] is applied once, in order, inside a
//! transaction. Never edit an existing migration; append a new one.

use rusqlite::Connection;

use crate::error::{LibraryError, Result};

pub(crate) const MIGRATIONS: &[&str] = &[
    // v1: initial schema.
    r"
    CREATE TABLE media (
        seq            INTEGER PRIMARY KEY,
        id             TEXT    NOT NULL UNIQUE,
        path           TEXT    NOT NULL,
        file_name      TEXT    NOT NULL,
        folder         TEXT    NOT NULL,
        fp_hash        INTEGER NOT NULL,
        size           INTEGER NOT NULL,
        mtime_ms       INTEGER NOT NULL,
        kind           TEXT    NOT NULL,
        cloud_state    TEXT    NOT NULL DEFAULT 'unknown',
        probe_state    TEXT    NOT NULL DEFAULT 'pending',
        probe_error    TEXT,
        width          INTEGER,
        height         INTEGER,
        rotation       INTEGER NOT NULL DEFAULT 0,
        duration       INTEGER,
        fps_num        INTEGER,
        fps_den        INTEGER,
        has_audio      INTEGER NOT NULL DEFAULT 0,
        transfer       TEXT    NOT NULL DEFAULT 'sdr',
        captured_at_ms INTEGER,
        codec          TEXT    NOT NULL DEFAULT '',
        added_at_ms    INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX media_fingerprint ON media(fp_hash, size);
    CREATE INDEX media_sort_time ON media(COALESCE(captured_at_ms, mtime_ms));
    CREATE INDEX media_kind ON media(kind);
    CREATE INDEX media_name ON media(file_name COLLATE NOCASE);
    CREATE INDEX media_added ON media(added_at_ms);

    CREATE VIRTUAL TABLE media_fts USING fts5(
        file_name, folder,
        content='media', content_rowid='seq',
        tokenize='trigram'
    );
    CREATE TRIGGER media_ai AFTER INSERT ON media BEGIN
        INSERT INTO media_fts(rowid, file_name, folder) VALUES (new.seq, new.file_name, new.folder);
    END;
    CREATE TRIGGER media_ad AFTER DELETE ON media BEGIN
        INSERT INTO media_fts(media_fts, rowid, file_name, folder) VALUES ('delete', old.seq, old.file_name, old.folder);
    END;
    CREATE TRIGGER media_au AFTER UPDATE OF file_name, folder ON media BEGIN
        INSERT INTO media_fts(media_fts, rowid, file_name, folder) VALUES ('delete', old.seq, old.file_name, old.folder);
        INSERT INTO media_fts(rowid, file_name, folder) VALUES (new.seq, new.file_name, new.folder);
    END;

    CREATE TABLE thumb (
        media_seq       INTEGER NOT NULL REFERENCES media(seq) ON DELETE CASCADE,
        level           INTEGER NOT NULL,
        path            TEXT    NOT NULL,
        width           INTEGER NOT NULL,
        height          INTEGER NOT NULL,
        generated_at_ms INTEGER NOT NULL,
        PRIMARY KEY (media_seq, level)
    ) WITHOUT ROWID;
    ",
];

pub(crate) const CURRENT_VERSION: u32 = MIGRATIONS.len() as u32;

pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    let found: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if found > CURRENT_VERSION {
        return Err(LibraryError::SchemaTooNew {
            found,
            supported: CURRENT_VERSION,
        });
    }
    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        // user_version cannot be bound as a parameter.
        tx.execute_batch(&format!("PRAGMA user_version = {}", idx + 1))?;
        tx.commit()?;
    }
    Ok(())
}
