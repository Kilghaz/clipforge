//! The SQLite catalogue.

use std::path::{Path, PathBuf};

use clipforge_core::{FrameRate, MediaId, Ticks};
use clipforge_media::{ColorTransfer, MediaInfo, MediaKind, Rotation};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{LibraryError, Result};
use crate::fingerprint::Fingerprint;
use crate::query::{Query, Sort};
use crate::record::{CloudState, MediaRecord, NewMedia, ProbeState, ThumbLevel, ThumbRecord};
use crate::schema;

/// Outcome of registering a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Added {
    /// The file was new.
    New(MediaRecord),
    /// A file with the same fingerprint already existed; nothing changed.
    Existing(MediaRecord),
}

impl Added {
    #[must_use]
    pub fn record(&self) -> &MediaRecord {
        match self {
            Added::New(r) | Added::Existing(r) => r,
        }
    }

    #[must_use]
    pub fn is_new(&self) -> bool {
        matches!(self, Added::New(_))
    }
}

/// Handle to the catalogue database. Not `Sync`; open one per thread or
/// funnel access through the owning job.
#[derive(Debug)]
pub struct Catalogue {
    conn: Connection,
}

impl Catalogue {
    /// Opens (and creates or migrates) the catalogue at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// A private in-memory catalogue, for tests and previews.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA temp_store = MEMORY;
             PRAGMA busy_timeout = 5000;",
        )?;
        schema::migrate(&mut conn)?;
        Ok(Catalogue { conn })
    }

    /// Schema version of the open database.
    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    /// Registers a file, deduplicating by fingerprint.
    pub fn add(&mut self, new: &NewMedia, now_ms: i64) -> Result<Added> {
        Self::insert(&self.conn, new, now_ms)
    }

    /// Registers many files in one transaction. Returns one outcome per input.
    pub fn add_many(&mut self, items: &[NewMedia], now_ms: i64) -> Result<Vec<Added>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(Self::insert(&tx, item, now_ms)?);
        }
        tx.commit()?;
        Ok(out)
    }

    fn insert(conn: &Connection, new: &NewMedia, now_ms: i64) -> Result<Added> {
        if let Some(existing) = Self::find_fp(conn, new.fingerprint)? {
            return Ok(Added::Existing(existing));
        }
        let id = MediaId::new();
        let (file_name, folder) = split_path(&new.path);
        conn.execute(
            "INSERT INTO media (id, path, file_name, folder, fp_hash, size, mtime_ms, kind, cloud_state, added_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id.to_string(),
                path_to_db(&new.path),
                file_name,
                folder,
                hash_to_db(new.fingerprint.hash),
                size_to_db(new.fingerprint.size),
                new.mtime_ms,
                kind_to_db(new.kind),
                new.cloud_state.as_str(),
                now_ms,
            ],
        )?;
        Self::get_from(conn, id).map(Added::New)
    }

    fn get_from(conn: &Connection, id: MediaId) -> Result<MediaRecord> {
        conn.query_row(
            &format!("SELECT {COLUMNS} FROM media WHERE id = ?1"),
            params![id.to_string()],
            row_to_record,
        )
        .optional()?
        .ok_or(LibraryError::NotFound(id))
    }

    fn find_fp(conn: &Connection, fp: Fingerprint) -> Result<Option<MediaRecord>> {
        Ok(conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM media WHERE fp_hash = ?1 AND size = ?2"),
                params![hash_to_db(fp.hash), size_to_db(fp.size)],
                row_to_record,
            )
            .optional()?)
    }

    pub fn get(&self, id: MediaId) -> Result<MediaRecord> {
        Self::get_from(&self.conn, id)
    }

    pub fn find_by_fingerprint(&self, fp: Fingerprint) -> Result<Option<MediaRecord>> {
        Self::find_fp(&self.conn, fp)
    }

    pub fn find_by_path(&self, path: &Path) -> Result<Option<MediaRecord>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM media WHERE path = ?1"),
                params![path_to_db(path)],
                row_to_record,
            )
            .optional()?)
    }

    /// Stores the result of a successful probe.
    pub fn set_info(&mut self, id: MediaId, info: &MediaInfo) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE media SET probe_state = 'done', probe_error = NULL, kind = ?2, width = ?3, height = ?4,
                    rotation = ?5, duration = ?6, fps_num = ?7, fps_den = ?8, has_audio = ?9,
                    transfer = ?10, captured_at_ms = ?11, codec = ?12
             WHERE id = ?1",
            params![
                id.to_string(),
                kind_to_db(info.kind),
                info.width,
                info.height,
                info.rotation.degrees(),
                info.duration.map(Ticks::flicks),
                info.frame_rate.map(FrameRate::numerator),
                info.frame_rate.map(FrameRate::denominator),
                info.has_audio,
                transfer_to_db(info.transfer),
                info.captured_at_ms,
                info.codec,
            ],
        )?;
        if changed == 0 {
            Err(LibraryError::NotFound(id))
        } else {
            Ok(())
        }
    }

    /// Records that probing failed. The item stays in the library so the
    /// user can see and remove it.
    pub fn set_probe_failed(&mut self, id: MediaId, message: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE media SET probe_state = 'failed', probe_error = ?2 WHERE id = ?1",
            params![id.to_string(), message],
        )?;
        if changed == 0 {
            Err(LibraryError::NotFound(id))
        } else {
            Ok(())
        }
    }

    pub fn set_cloud_state(&mut self, id: MediaId, state: CloudState) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE media SET cloud_state = ?2 WHERE id = ?1",
            params![id.to_string(), state.as_str()],
        )?;
        if changed == 0 {
            Err(LibraryError::NotFound(id))
        } else {
            Ok(())
        }
    }

    /// Replaces a provisional fingerprint with the real one once the bytes
    /// are on disk (cloud download). Fails with a constraint error if the
    /// same file is already in the library under another row.
    pub fn set_fingerprint(&mut self, id: MediaId, fp: Fingerprint, mtime_ms: i64) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE media SET fp_hash = ?2, size = ?3, mtime_ms = ?4 WHERE id = ?1",
            params![
                id.to_string(),
                hash_to_db(fp.hash),
                size_to_db(fp.size),
                mtime_ms
            ],
        )?;
        if changed == 0 {
            Err(LibraryError::NotFound(id))
        } else {
            Ok(())
        }
    }

    /// Points an item at a new location (after the user moved the file).
    pub fn relink(&mut self, id: MediaId, new_path: &Path, mtime_ms: i64) -> Result<()> {
        let (file_name, folder) = split_path(new_path);
        let changed = self.conn.execute(
            "UPDATE media SET path = ?2, file_name = ?3, folder = ?4, mtime_ms = ?5 WHERE id = ?1",
            params![
                id.to_string(),
                path_to_db(new_path),
                file_name,
                folder,
                mtime_ms
            ],
        )?;
        if changed == 0 {
            Err(LibraryError::NotFound(id))
        } else {
            Ok(())
        }
    }

    /// Removes items and their thumbnail records (not the files).
    pub fn remove(&mut self, ids: &[MediaId]) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut removed = 0;
        for id in ids {
            removed += tx.execute("DELETE FROM media WHERE id = ?1", params![id.to_string()])?;
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Items matching `query`, in the requested order.
    pub fn query(&self, query: &Query) -> Result<Vec<MediaRecord>> {
        let (where_sql, binds) = build_where(query);
        let order = order_sql(query.sort, query.descending);
        let mut sql = format!("SELECT {COLUMNS} FROM media {where_sql} ORDER BY {order}");
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {limit} OFFSET {}", query.offset));
        } else if query.offset > 0 {
            sql.push_str(&format!(" LIMIT -1 OFFSET {}", query.offset));
        }
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), row_to_record)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Ids only, for building a virtualised grid model cheaply.
    pub fn query_ids(&self, query: &Query) -> Result<Vec<MediaId>> {
        let (where_sql, binds) = build_where(query);
        let order = order_sql(query.sort, query.descending);
        let sql = format!("SELECT id FROM media {where_sql} ORDER BY {order}");
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), |r| {
            r.get::<_, String>(0)
        })?;
        rows.map(|r| r.map_err(LibraryError::from).and_then(|s| parse_id(&s)))
            .collect()
    }

    /// Number of items matching `query` (ignoring paging).
    pub fn count(&self, query: &Query) -> Result<u64> {
        let (where_sql, binds) = build_where(query);
        let sql = format!("SELECT COUNT(*) FROM media {where_sql}");
        let mut stmt = self.conn.prepare_cached(&sql)?;
        Ok(stmt
            .query_row(rusqlite::params_from_iter(binds.iter()), |r| {
                r.get::<_, i64>(0)
            })?
            .max(0) as u64)
    }

    /// Ids of items still waiting for a probe, oldest first.
    pub fn pending_probe_ids(&self, limit: u64) -> Result<Vec<MediaId>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id FROM media WHERE probe_state = 'pending' ORDER BY seq LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| r.get::<_, String>(0))?;
        rows.map(|r| r.map_err(LibraryError::from).and_then(|s| parse_id(&s)))
            .collect()
    }

    pub fn set_thumb(&mut self, thumb: &ThumbRecord) -> Result<()> {
        let changed = self.conn.execute(
            "INSERT INTO thumb (media_seq, level, path, width, height, generated_at_ms)
             SELECT seq, ?2, ?3, ?4, ?5, ?6 FROM media WHERE id = ?1
             ON CONFLICT(media_seq, level) DO UPDATE SET
                path = excluded.path, width = excluded.width, height = excluded.height,
                generated_at_ms = excluded.generated_at_ms",
            params![
                thumb.media_id.to_string(),
                thumb.level.as_i64(),
                path_to_db(&thumb.path),
                thumb.width,
                thumb.height,
                thumb.generated_at_ms
            ],
        )?;
        if changed == 0 {
            Err(LibraryError::NotFound(thumb.media_id))
        } else {
            Ok(())
        }
    }

    pub fn thumb(&self, id: MediaId, level: ThumbLevel) -> Result<Option<ThumbRecord>> {
        Ok(self
            .conn
            .query_row(
                "SELECT t.path, t.width, t.height, t.generated_at_ms FROM thumb t
                 JOIN media m ON m.seq = t.media_seq WHERE m.id = ?1 AND t.level = ?2",
                params![id.to_string(), level.as_i64()],
                |r| {
                    Ok(ThumbRecord {
                        media_id: id,
                        level,
                        path: db_to_path(r.get::<_, String>(0)?),
                        width: r.get(1)?,
                        height: r.get(2)?,
                        generated_at_ms: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// All thumbnails of an item, smallest level first.
    pub fn thumbs(&self, id: MediaId) -> Result<Vec<ThumbRecord>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT t.level, t.path, t.width, t.height, t.generated_at_ms FROM thumb t
             JOIN media m ON m.seq = t.media_seq WHERE m.id = ?1 ORDER BY t.level",
        )?;
        let rows = stmt.query_map(params![id.to_string()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, u32>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (level, path, width, height, generated_at_ms) = row?;
            let level = ThumbLevel::from_i64(level)
                .ok_or_else(|| LibraryError::Corrupt(format!("unknown thumb level {level}")))?;
            out.push(ThumbRecord {
                media_id: id,
                level,
                path: db_to_path(path),
                width,
                height,
                generated_at_ms,
            });
        }
        Ok(out)
    }

    /// Total number of items.
    pub fn len(&self) -> Result<u64> {
        self.count(&Query::all())
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

const COLUMNS: &str = "id, path, fp_hash, size, mtime_ms, kind, cloud_state, probe_state, probe_error, width, height, \
                       rotation, duration, fps_num, fps_den, has_audio, transfer, captured_at_ms, codec, added_at_ms";

fn row_to_record(r: &Row<'_>) -> rusqlite::Result<MediaRecord> {
    let id: String = r.get(0)?;
    let id = parse_id(&id).map_err(|_| rusqlite::Error::InvalidQuery)?;
    let path = db_to_path(r.get::<_, String>(1)?);
    let fingerprint = Fingerprint {
        hash: db_to_hash(r.get::<_, i64>(2)?),
        size: db_to_size(r.get::<_, i64>(3)?),
    };
    let mtime_ms: i64 = r.get(4)?;
    let kind = kind_from_db(&r.get::<_, String>(5)?);
    let cloud_state = CloudState::parse(&r.get::<_, String>(6)?).unwrap_or_default();
    let probe_state: String = r.get(7)?;
    let probe_error: Option<String> = r.get(8)?;
    let probe = match probe_state.as_str() {
        "done" => ProbeState::Done,
        "failed" => ProbeState::Failed(probe_error.unwrap_or_default()),
        _ => ProbeState::Pending,
    };
    let info = if probe == ProbeState::Done {
        let fps_num: Option<u32> = r.get(13)?;
        let fps_den: Option<u32> = r.get(14)?;
        Some(MediaInfo {
            kind,
            width: r.get(9)?,
            height: r.get(10)?,
            rotation: Rotation::from_degrees(r.get::<_, i32>(11)?),
            duration: r.get::<_, Option<i64>>(12)?.map(Ticks::from_flicks),
            frame_rate: fps_num.zip(fps_den).and_then(|(n, d)| FrameRate::new(n, d)),
            has_audio: r.get(15)?,
            transfer: transfer_from_db(&r.get::<_, String>(16)?),
            captured_at_ms: r.get(17)?,
            codec: r.get(18)?,
        })
    } else {
        None
    };
    Ok(MediaRecord {
        id,
        path,
        fingerprint,
        mtime_ms,
        kind,
        cloud_state,
        probe,
        info,
        added_at_ms: r.get(19)?,
    })
}

fn build_where(q: &Query) -> (String, Vec<rusqlite::types::Value>) {
    use rusqlite::types::Value;
    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<Value> = Vec::new();

    if let Some(kinds) = &q.kinds {
        if kinds.is_empty() {
            clauses.push("0".into());
        } else {
            let placeholders: Vec<String> = kinds
                .iter()
                .map(|k| {
                    binds.push(Value::Text(kind_to_db(*k).to_owned()));
                    format!("?{}", binds.len())
                })
                .collect();
            clauses.push(format!("kind IN ({})", placeholders.join(", ")));
        }
    }
    if let Some(text) = &q.text {
        let text = text.trim();
        if text.chars().count() >= 3 {
            // FTS5 trigram: quote the phrase, escape embedded quotes.
            binds.push(Value::Text(format!("\"{}\"", text.replace('"', "\"\""))));
            clauses.push(format!(
                "seq IN (SELECT rowid FROM media_fts WHERE media_fts MATCH ?{})",
                binds.len()
            ));
        } else {
            binds.push(Value::Text(format!("%{}%", like_escape(text))));
            let n = binds.len();
            clauses.push(format!(
                "(file_name LIKE ?{n} ESCAPE '\\' OR folder LIKE ?{n} ESCAPE '\\')"
            ));
        }
    }
    if let Some((from, to)) = q.time_range_ms {
        binds.push(Value::Integer(from));
        let a = binds.len();
        binds.push(Value::Integer(to));
        let b = binds.len();
        clauses.push(format!(
            "COALESCE(captured_at_ms, mtime_ms) BETWEEN ?{a} AND ?{b}"
        ));
    }
    if q.only_hdr {
        clauses.push("transfer <> 'sdr'".into());
    }
    if q.only_failed {
        clauses.push("probe_state = 'failed'".into());
    }
    if q.only_placeholders {
        clauses.push("cloud_state = 'placeholder'".into());
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    (where_sql, binds)
}

fn order_sql(sort: Sort, descending: bool) -> String {
    let dir = if descending { "DESC" } else { "ASC" };
    let primary = match sort {
        Sort::CapturedAt => "COALESCE(captured_at_ms, mtime_ms)",
        Sort::Name => "file_name COLLATE NOCASE",
        Sort::Kind => "kind",
        Sort::AddedAt => "added_at_ms",
        Sort::Duration => "COALESCE(duration, 0)",
        Sort::FileSize => "size",
    };
    // seq as tie-breaker keeps paging stable.
    format!("{primary} {dir}, seq {dir}")
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn split_path(path: &Path) -> (String, String) {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    (file_name, folder)
}

fn path_to_db(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn db_to_path(s: String) -> PathBuf {
    PathBuf::from(s)
}

fn parse_id(s: &str) -> Result<MediaId> {
    s.parse()
        .map_err(|_| LibraryError::Corrupt(format!("bad media id {s:?}")))
}

// SQLite integers are signed; store u64 bit patterns as i64.
#[allow(clippy::cast_possible_wrap)]
fn hash_to_db(h: u64) -> i64 {
    h as i64
}
#[allow(clippy::cast_sign_loss)]
fn db_to_hash(v: i64) -> u64 {
    v as u64
}
#[allow(clippy::cast_possible_wrap)]
fn size_to_db(s: u64) -> i64 {
    s as i64
}
#[allow(clippy::cast_sign_loss)]
fn db_to_size(v: i64) -> u64 {
    v as u64
}

fn kind_to_db(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Photo => "photo",
        MediaKind::Video => "video",
        MediaKind::Audio => "audio",
        MediaKind::Unknown => "unknown",
    }
}

fn kind_from_db(s: &str) -> MediaKind {
    match s {
        "photo" => MediaKind::Photo,
        "video" => MediaKind::Video,
        "audio" => MediaKind::Audio,
        _ => MediaKind::Unknown,
    }
}

fn transfer_to_db(t: ColorTransfer) -> &'static str {
    match t {
        ColorTransfer::Sdr => "sdr",
        ColorTransfer::Hlg => "hlg",
        ColorTransfer::Pq => "pq",
        ColorTransfer::GainMap => "gain_map",
    }
}

fn transfer_from_db(s: &str) -> ColorTransfer {
    match s {
        "hlg" => ColorTransfer::Hlg,
        "pq" => ColorTransfer::Pq,
        "gain_map" => ColorTransfer::GainMap,
        _ => ColorTransfer::Sdr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_media(name: &str, hash: u64, kind: MediaKind) -> NewMedia {
        NewMedia {
            path: PathBuf::from(format!("/photos/2024/{name}")),
            fingerprint: Fingerprint {
                hash,
                size: 1000 + hash,
            },
            mtime_ms: 1_700_000_000_000 + hash as i64,
            kind,
            cloud_state: CloudState::Local,
        }
    }

    fn info(kind: MediaKind, captured_at_ms: Option<i64>) -> MediaInfo {
        MediaInfo {
            kind,
            width: Some(4032),
            height: Some(3024),
            rotation: Rotation::Cw90,
            duration: (kind == MediaKind::Video).then(|| Ticks::from_seconds(12)),
            frame_rate: (kind == MediaKind::Video).then_some(FrameRate::FPS_29_97),
            has_audio: kind == MediaKind::Video,
            transfer: ColorTransfer::Hlg,
            captured_at_ms,
            codec: "hevc".into(),
        }
    }

    #[test]
    fn fresh_catalogue_is_empty_and_at_current_version() {
        let cat = Catalogue::open_in_memory().unwrap();
        assert!(cat.is_empty().unwrap());
        assert_eq!(cat.schema_version().unwrap(), schema::CURRENT_VERSION);
    }

    #[test]
    fn add_then_get_round_trips_every_field() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let added = cat
            .add(&new_media("a.heic", 1, MediaKind::Photo), 42)
            .unwrap();
        assert!(added.is_new());
        let rec = cat.get(added.record().id).unwrap();
        assert_eq!(rec.path, PathBuf::from("/photos/2024/a.heic"));
        assert_eq!(rec.file_name(), "a.heic");
        assert_eq!(
            rec.fingerprint,
            Fingerprint {
                hash: 1,
                size: 1001
            }
        );
        assert_eq!(rec.kind, MediaKind::Photo);
        assert_eq!(rec.probe, ProbeState::Pending);
        assert_eq!(rec.info, None);
        assert_eq!(rec.cloud_state, CloudState::Local);
        assert_eq!(rec.added_at_ms, 42);
        assert_eq!(rec.sort_time_ms(), rec.mtime_ms);
    }

    #[test]
    fn duplicate_fingerprint_returns_existing() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let first = cat
            .add(&new_media("a.jpg", 7, MediaKind::Photo), 1)
            .unwrap();
        let mut copy = new_media("copy-of-a.jpg", 7, MediaKind::Photo);
        copy.path = PathBuf::from("/elsewhere/copy.jpg");
        let second = cat.add(&copy, 2).unwrap();
        assert!(!second.is_new());
        assert_eq!(second.record().id, first.record().id);
        assert_eq!(cat.len().unwrap(), 1);
    }

    #[test]
    fn set_info_persists_probe_result() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let id = cat
            .add(&new_media("v.mov", 3, MediaKind::Video), 1)
            .unwrap()
            .record()
            .id;
        let i = info(MediaKind::Video, Some(1_600_000_000_000));
        cat.set_info(id, &i).unwrap();
        let rec = cat.get(id).unwrap();
        assert_eq!(rec.probe, ProbeState::Done);
        assert_eq!(rec.info.as_ref(), Some(&i));
        assert_eq!(rec.sort_time_ms(), 1_600_000_000_000);
    }

    #[test]
    fn failed_probe_keeps_item_and_message() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let id = cat
            .add(&new_media("broken.jpg", 4, MediaKind::Photo), 1)
            .unwrap()
            .record()
            .id;
        cat.set_probe_failed(id, "not a jpeg").unwrap();
        assert_eq!(
            cat.get(id).unwrap().probe,
            ProbeState::Failed("not a jpeg".into())
        );
        assert_eq!(
            cat.count(&Query {
                only_failed: true,
                ..Query::all()
            })
            .unwrap(),
            1
        );
        assert_eq!(cat.pending_probe_ids(10).unwrap(), Vec::<MediaId>::new());
    }

    #[test]
    fn unknown_id_is_not_found() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let id = MediaId::new();
        assert!(matches!(cat.get(id), Err(LibraryError::NotFound(_))));
        assert!(matches!(
            cat.set_probe_failed(id, "x"),
            Err(LibraryError::NotFound(_))
        ));
        assert!(matches!(
            cat.relink(id, Path::new("/x"), 0),
            Err(LibraryError::NotFound(_))
        ));
    }

    #[test]
    fn relink_updates_path_and_search_index() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let id = cat
            .add(&new_media("holiday.jpg", 5, MediaKind::Photo), 1)
            .unwrap()
            .record()
            .id;
        cat.relink(id, Path::new("/moved/beach.jpg"), 99).unwrap();
        let rec = cat.get(id).unwrap();
        assert_eq!(rec.path, PathBuf::from("/moved/beach.jpg"));
        assert_eq!(rec.mtime_ms, 99);
        assert_eq!(
            cat.find_by_path(Path::new("/moved/beach.jpg"))
                .unwrap()
                .map(|r| r.id),
            Some(id)
        );
        assert_eq!(cat.query(&Query::all().text("beach")).unwrap().len(), 1);
        assert_eq!(cat.query(&Query::all().text("holiday")).unwrap().len(), 0);
    }

    #[test]
    fn remove_deletes_items_and_thumbs() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let id = cat
            .add(&new_media("a.jpg", 6, MediaKind::Photo), 1)
            .unwrap()
            .record()
            .id;
        cat.set_thumb(&ThumbRecord {
            media_id: id,
            level: ThumbLevel::Small,
            path: "/cache/a/256.jpg".into(),
            width: 256,
            height: 192,
            generated_at_ms: 5,
        })
        .unwrap();
        assert_eq!(cat.remove(&[id]).unwrap(), 1);
        assert!(cat.is_empty().unwrap());
        assert!(cat.thumb(id, ThumbLevel::Small).unwrap().is_none());
        assert_eq!(cat.remove(&[id]).unwrap(), 0);
    }

    #[test]
    fn thumbs_upsert_and_list_by_level() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let id = cat
            .add(&new_media("a.jpg", 8, MediaKind::Photo), 1)
            .unwrap()
            .record()
            .id;
        for (level, w) in [(ThumbLevel::Preview, 1280), (ThumbLevel::Small, 256)] {
            cat.set_thumb(&ThumbRecord {
                media_id: id,
                level,
                path: format!("/cache/{w}.jpg").into(),
                width: w,
                height: w * 3 / 4,
                generated_at_ms: 1,
            })
            .unwrap();
        }
        let listed = cat.thumbs(id).unwrap();
        assert_eq!(
            listed.iter().map(|t| t.level).collect::<Vec<_>>(),
            [ThumbLevel::Small, ThumbLevel::Preview]
        );
        // Upsert replaces.
        cat.set_thumb(&ThumbRecord {
            media_id: id,
            level: ThumbLevel::Small,
            path: "/cache/new.jpg".into(),
            width: 256,
            height: 256,
            generated_at_ms: 2,
        })
        .unwrap();
        let small = cat.thumb(id, ThumbLevel::Small).unwrap().unwrap();
        assert_eq!(small.path, PathBuf::from("/cache/new.jpg"));
        assert_eq!(small.generated_at_ms, 2);
        assert_eq!(cat.thumbs(id).unwrap().len(), 2);
    }

    #[test]
    fn thumb_for_unknown_media_is_rejected() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let err = cat
            .set_thumb(&ThumbRecord {
                media_id: MediaId::new(),
                level: ThumbLevel::Small,
                path: "/x".into(),
                width: 1,
                height: 1,
                generated_at_ms: 0,
            })
            .unwrap_err();
        assert!(matches!(err, LibraryError::NotFound(_)));
    }

    fn seeded() -> (Catalogue, Vec<MediaId>) {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let items = [
            ("IMG_0001.HEIC", MediaKind::Photo, Some(3_000)),
            ("IMG_0002.jpg", MediaKind::Photo, Some(1_000)),
            ("clip_beach.mov", MediaKind::Video, Some(2_000)),
            ("song.m4a", MediaKind::Audio, None),
        ];
        let mut ids = Vec::new();
        for (i, (name, kind, captured)) in items.iter().enumerate() {
            let mut nm = new_media(name, i as u64 + 10, *kind);
            nm.mtime_ms = 500 + i as i64; // song sorts by mtime = 503
            let id = cat.add(&nm, i as i64).unwrap().record().id;
            if *kind != MediaKind::Audio {
                let mut inf = info(*kind, *captured);
                if *kind == MediaKind::Photo && i == 1 {
                    inf.transfer = ColorTransfer::Sdr;
                }
                cat.set_info(id, &inf).unwrap();
            }
            ids.push(id);
        }
        (cat, ids)
    }

    fn names(recs: &[MediaRecord]) -> Vec<String> {
        recs.iter().map(MediaRecord::file_name).collect()
    }

    #[test]
    fn default_query_sorts_newest_capture_first_with_mtime_fallback() {
        let (cat, _) = seeded();
        assert_eq!(
            names(&cat.query(&Query::all()).unwrap()),
            [
                "IMG_0001.HEIC",
                "clip_beach.mov",
                "IMG_0002.jpg",
                "song.m4a"
            ]
        );
        let asc = cat
            .query(&Query::all().sort(Sort::CapturedAt, false))
            .unwrap();
        assert_eq!(
            names(&asc),
            [
                "song.m4a",
                "IMG_0002.jpg",
                "clip_beach.mov",
                "IMG_0001.HEIC"
            ]
        );
    }

    #[test]
    fn sort_by_name_is_case_insensitive() {
        let (cat, _) = seeded();
        let by_name = cat.query(&Query::all().sort(Sort::Name, false)).unwrap();
        assert_eq!(
            names(&by_name),
            [
                "clip_beach.mov",
                "IMG_0001.HEIC",
                "IMG_0002.jpg",
                "song.m4a"
            ]
        );
    }

    #[test]
    fn filters_by_kind_hdr_and_time() {
        let (cat, _) = seeded();
        let photos = cat.query(&Query::all().kinds([MediaKind::Photo])).unwrap();
        assert_eq!(names(&photos), ["IMG_0001.HEIC", "IMG_0002.jpg"]);
        let av = cat
            .query(&Query::all().kinds([MediaKind::Video, MediaKind::Audio]))
            .unwrap();
        assert_eq!(av.len(), 2);
        assert_eq!(cat.count(&Query::all().kinds([])).unwrap(), 0);

        let hdr = cat
            .query(&Query {
                only_hdr: true,
                ..Query::all()
            })
            .unwrap();
        assert_eq!(names(&hdr), ["IMG_0001.HEIC", "clip_beach.mov"]);

        let range = cat
            .query(&Query {
                time_range_ms: Some((1_500, 2_500)),
                ..Query::all()
            })
            .unwrap();
        assert_eq!(names(&range), ["clip_beach.mov"]);
    }

    #[test]
    fn text_search_matches_substrings_case_insensitively() {
        let (cat, _) = seeded();
        assert_eq!(
            names(&cat.query(&Query::all().text("img_000")).unwrap()),
            ["IMG_0001.HEIC", "IMG_0002.jpg"]
        );
        assert_eq!(
            names(&cat.query(&Query::all().text("BEACH")).unwrap()),
            ["clip_beach.mov"]
        );
        assert_eq!(
            names(&cat.query(&Query::all().text("2024")).unwrap()).len(),
            4,
            "folder is searchable"
        );
        // Short queries fall back to LIKE.
        assert_eq!(
            names(&cat.query(&Query::all().text("so")).unwrap()),
            ["song.m4a"]
        );
        assert_eq!(
            names(&cat.query(&Query::all().text("%")).unwrap()).len(),
            0,
            "wildcards are literal"
        );
        assert_eq!(
            cat.query(&Query::all().text("   ")).unwrap().len(),
            4,
            "blank text means no filter"
        );
        assert_eq!(cat.query(&Query::all().text("nomatch")).unwrap().len(), 0);
    }

    #[test]
    fn paging_and_ids_agree_with_full_query() {
        let (cat, _) = seeded();
        let all = cat.query(&Query::all()).unwrap();
        let ids = cat.query_ids(&Query::all()).unwrap();
        assert_eq!(all.iter().map(|r| r.id).collect::<Vec<_>>(), ids);
        let page = cat.query(&Query::all().page(1, 2)).unwrap();
        assert_eq!(names(&page), names(&all[1..3]));
        assert_eq!(
            cat.count(&Query::all().page(1, 2)).unwrap(),
            4,
            "count ignores paging"
        );
        let tail = cat
            .query(&Query {
                offset: 3,
                ..Query::all()
            })
            .unwrap();
        assert_eq!(tail.len(), 1);
    }

    #[test]
    fn pending_probe_ids_in_insertion_order() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let a = cat
            .add(&new_media("a.jpg", 1, MediaKind::Photo), 1)
            .unwrap()
            .record()
            .id;
        let b = cat
            .add(&new_media("b.jpg", 2, MediaKind::Photo), 1)
            .unwrap()
            .record()
            .id;
        let c = cat
            .add(&new_media("c.jpg", 3, MediaKind::Photo), 1)
            .unwrap()
            .record()
            .id;
        cat.set_info(b, &info(MediaKind::Photo, None)).unwrap();
        assert_eq!(cat.pending_probe_ids(10).unwrap(), [a, c]);
        assert_eq!(cat.pending_probe_ids(1).unwrap(), [a]);
    }

    #[test]
    fn add_many_is_atomic_and_deduplicates_within_batch() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let batch = vec![
            new_media("a.jpg", 1, MediaKind::Photo),
            new_media("b.jpg", 2, MediaKind::Photo),
            new_media("a-again.jpg", 1, MediaKind::Photo),
        ];
        let out = cat.add_many(&batch, 7).unwrap();
        assert_eq!(out.iter().filter(|a| a.is_new()).count(), 2);
        assert_eq!(out[2].record().id, out[0].record().id);
        assert_eq!(cat.len().unwrap(), 2);
    }

    #[test]
    fn persists_to_disk_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lib").join("catalogue.sqlite");
        let id = {
            let mut cat = Catalogue::open(&path).unwrap();
            cat.add(&new_media("a.jpg", 1, MediaKind::Photo), 1)
                .unwrap()
                .record()
                .id
        };
        let cat = Catalogue::open(&path).unwrap();
        assert_eq!(cat.get(id).unwrap().file_name(), "a.jpg");
        assert_eq!(cat.schema_version().unwrap(), schema::CURRENT_VERSION);
    }

    #[test]
    fn refuses_databases_from_the_future() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalogue.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA user_version = 999").unwrap();
        }
        assert!(matches!(
            Catalogue::open(&path),
            Err(LibraryError::SchemaTooNew { found: 999, .. })
        ));
    }

    #[test]
    fn cloud_state_can_change() {
        let mut cat = Catalogue::open_in_memory().unwrap();
        let mut nm = new_media("a.jpg", 1, MediaKind::Photo);
        nm.cloud_state = CloudState::Placeholder;
        let id = cat.add(&nm, 1).unwrap().record().id;
        assert_eq!(
            cat.count(&Query {
                only_placeholders: true,
                ..Query::all()
            })
            .unwrap(),
            1
        );
        cat.set_cloud_state(id, CloudState::Local).unwrap();
        assert_eq!(
            cat.count(&Query {
                only_placeholders: true,
                ..Query::all()
            })
            .unwrap(),
            0
        );
    }
}
