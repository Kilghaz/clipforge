//! Turning dropped paths into catalogue candidates.
//!
//! Enumeration is deliberately cheap: it reads directory entries and file
//! metadata but never file contents, so it is fast on network and cloud
//! volumes. Fingerprinting (which reads up to 2 MiB per file) happens in a
//! second step and is skipped for cloud placeholders.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use clipforge_media::MediaKind;
use clipforge_platform::{CloudStatus, cloud_status, icloud_stub};
use xxhash_rust::xxh3::xxh3_64;

use crate::fingerprint::Fingerprint;
use crate::record::{CloudState, NewMedia};

/// A file found during enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Path the item will be registered under. For iCloud stubs this is the
    /// target path (the real file's name), not the stub.
    pub path: PathBuf,
    /// Path to read metadata from (the stub for placeholders).
    pub source: PathBuf,
    pub kind: MediaKind,
    pub size: u64,
    pub mtime_ms: i64,
    pub cloud_state: CloudState,
}

/// Folders skipped during recursive enumeration.
const SKIPPED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".Trash",
    "$RECYCLE.BIN",
    "System Volume Information",
];

/// Expands files and folders (recursively) into supported media candidates.
/// Order is deterministic: sorted by path within each root.
#[must_use]
pub fn enumerate(roots: &[PathBuf]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for root in roots {
        if root.is_dir() {
            let walker = walkdir::WalkDir::new(root)
                .follow_links(false)
                .sort_by_file_name()
                .into_iter();
            for entry in walker
                .filter_entry(|e| !is_skipped_dir(e))
                .filter_map(std::result::Result::ok)
            {
                if entry.file_type().is_file()
                    && let Some(c) = candidate(entry.path())
                {
                    out.push(c);
                }
            }
        } else if let Some(c) = candidate(root) {
            out.push(c);
        }
    }
    out
}

fn is_skipped_dir(e: &walkdir::DirEntry) -> bool {
    e.file_type().is_dir()
        && e.file_name()
            .to_str()
            .is_some_and(|n| SKIPPED_DIRS.contains(&n) || (n.starts_with('.') && e.depth() > 0))
}

/// Classifies one file. Returns `None` for unsupported types, hidden files
/// and macOS resource forks.
#[must_use]
pub fn candidate(path: &Path) -> Option<Candidate> {
    let name = path.file_name()?.to_str()?;
    if let Some(stub) = icloud_stub(path) {
        let kind = MediaKind::from_path(&stub.target);
        if kind == MediaKind::Unknown {
            return None;
        }
        let meta = std::fs::symlink_metadata(path).ok()?;
        return Some(Candidate {
            path: stub.target,
            source: path.to_path_buf(),
            kind,
            size: stub.size.unwrap_or(0),
            mtime_ms: mtime_ms(&meta),
            cloud_state: CloudState::Placeholder,
        });
    }
    if name.starts_with('.') || name.starts_with("._") {
        return None;
    }
    let kind = MediaKind::from_path(path);
    if kind == MediaKind::Unknown {
        return None;
    }
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let cloud_state = match cloud_status(path) {
        CloudStatus::Local => CloudState::Local,
        CloudStatus::Placeholder => CloudState::Placeholder,
    };
    Some(Candidate {
        path: path.to_path_buf(),
        source: path.to_path_buf(),
        kind,
        size: meta.len(),
        mtime_ms: mtime_ms(&meta),
        cloud_state,
    })
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// Computes the fingerprint and builds the row to insert. Placeholders get a
/// provisional fingerprint from their identity rather than their bytes, so
/// registering them never triggers a download.
pub fn prepare(candidate: &Candidate) -> std::io::Result<NewMedia> {
    let fingerprint = if candidate.cloud_state == CloudState::Placeholder {
        provisional_fingerprint(candidate)
    } else {
        Fingerprint::from_path(&candidate.source)?
    };
    Ok(NewMedia {
        path: candidate.path.clone(),
        fingerprint,
        mtime_ms: candidate.mtime_ms,
        kind: candidate.kind,
        cloud_state: candidate.cloud_state,
    })
}

/// Deterministic stand-in fingerprint for files we must not read.
#[must_use]
pub fn provisional_fingerprint(c: &Candidate) -> Fingerprint {
    let mut key = c.path.to_string_lossy().into_owned().into_bytes();
    key.extend_from_slice(&c.size.to_le_bytes());
    key.extend_from_slice(&c.mtime_ms.to_le_bytes());
    Fingerprint {
        size: c.size,
        hash: xxh3_64(&key) ^ 0x8000_0000_0000_0000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
    }

    #[test]
    fn enumerates_fixture_folder_recursively_and_deterministically() {
        let a = enumerate(&[fixtures()]);
        let b = enumerate(&[fixtures()]);
        assert_eq!(a, b);
        let names: Vec<String> = a
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"photo_landscape.jpg".to_owned()));
        assert!(names.contains(&"video_hlg_hevc.mp4".to_owned()));
        assert!(names.contains(&"audio_mono.wav".to_owned()));
        assert!(
            names.contains(&"broken_truncated.jpg".to_owned()),
            "broken files are still candidates"
        );
        assert!(!names.contains(&"MANIFEST.txt".to_owned()));
        assert!(
            names.contains(&"photo_cloud_only.jpg".to_owned()),
            "icloud stub registered under target name"
        );
    }

    #[test]
    fn icloud_stub_becomes_placeholder_candidate() {
        let c = candidate(&fixtures().join(".photo_cloud_only.jpg.icloud")).unwrap();
        assert_eq!(c.cloud_state, CloudState::Placeholder);
        assert_eq!(c.kind, MediaKind::Photo);
        assert_eq!(c.size, 2_451_208);
        assert_eq!(c.path.file_name().unwrap(), "photo_cloud_only.jpg");
        assert!(c.source.ends_with(".photo_cloud_only.jpg.icloud"));
        let nm = prepare(&c).unwrap();
        assert_eq!(nm.fingerprint.size, 2_451_208);
        assert_eq!(
            nm.fingerprint,
            provisional_fingerprint(&c),
            "no bytes read for placeholders"
        );
    }

    #[test]
    fn single_files_and_unsupported_paths() {
        let jpg = fixtures().join("photo_landscape.jpg");
        let list = enumerate(&[
            jpg.clone(),
            fixtures().join("MANIFEST.txt"),
            PathBuf::from("/nope/missing.jpg"),
        ]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, jpg);
        assert_eq!(list[0].cloud_state, CloudState::Local);
        assert!(list[0].size > 0);
        assert!(list[0].mtime_ms > 0);
        assert!(candidate(Path::new("/x/.hidden.jpg")).is_none());
        assert!(candidate(Path::new("/x/._resource.jpg")).is_none());
    }

    #[test]
    fn skips_hidden_and_tool_directories() {
        let dir = tempfile::tempdir().unwrap();
        for sub in [".git", "node_modules", ".hidden", "ok"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
            std::fs::write(dir.path().join(sub).join("a.jpg"), b"x").unwrap();
        }
        let list = enumerate(&[dir.path().to_path_buf()]);
        assert_eq!(list.len(), 1);
        assert!(list[0].path.starts_with(dir.path().join("ok")));
    }

    #[test]
    fn prepare_reads_real_fingerprint_for_local_files() {
        let c = candidate(&fixtures().join("photo_landscape.jpg")).unwrap();
        let nm = prepare(&c).unwrap();
        assert_eq!(nm.fingerprint, Fingerprint::from_path(&c.path).unwrap());
        assert_eq!(nm.kind, MediaKind::Photo);
    }
}
