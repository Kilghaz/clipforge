//! Content-addressed thumbnail cache on disk.
//!
//! Layout: `<root>/<hh>/<fingerprint>/<edge>.jpg` where `hh` are the first
//! two hex digits of the fingerprint (keeps directories small). Because the
//! key is the content fingerprint, a file that is linked twice or moved
//! never gets a second thumbnail.

use std::path::{Path, PathBuf};

use clipforge_media::DecodedImage;

use crate::error::Result;
use crate::fingerprint::Fingerprint;
use crate::record::ThumbLevel;

/// JPEG quality for cached thumbnails.
const JPEG_QUALITY: u8 = 82;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThumbCache {
    root: PathBuf,
}

impl ThumbCache {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        ThumbCache { root }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a thumbnail lives (whether or not it exists yet).
    #[must_use]
    pub fn path_for(&self, fp: Fingerprint, level: ThumbLevel) -> PathBuf {
        let hex = fp.to_hex();
        self.root
            .join(&hex[..2])
            .join(&hex)
            .join(format!("{}.jpg", level.long_edge()))
    }

    #[must_use]
    pub fn exists(&self, fp: Fingerprint, level: ThumbLevel) -> bool {
        self.path_for(fp, level).is_file()
    }

    /// Encodes and stores a thumbnail atomically. Returns its path.
    pub fn write(
        &self,
        fp: Fingerprint,
        level: ThumbLevel,
        image: &DecodedImage,
    ) -> Result<PathBuf> {
        let path = self.path_for(fp, level);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = image
            .to_jpeg(JPEG_QUALITY)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }

    /// Removes every cached level of one fingerprint.
    pub fn remove(&self, fp: Fingerprint) -> Result<()> {
        let hex = fp.to_hex();
        let dir = self.root.join(&hex[..2]).join(&hex);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Total bytes used by the cache.
    pub fn size_on_disk(&self) -> Result<u64> {
        Ok(self.entries()?.iter().map(|e| e.size).sum())
    }

    /// Deletes least recently modified thumbnails until the cache is at most
    /// `max_bytes`. Returns the number of files removed.
    pub fn evict_to(&self, max_bytes: u64) -> Result<usize> {
        let mut entries = self.entries()?;
        let mut total: u64 = entries.iter().map(|e| e.size).sum();
        if total <= max_bytes {
            return Ok(0);
        }
        entries.sort_by_key(|e| e.modified);
        let mut removed = 0;
        for e in entries {
            if total <= max_bytes {
                break;
            }
            if std::fs::remove_file(&e.path).is_ok() {
                total = total.saturating_sub(e.size);
                removed += 1;
                // Drop empty parents so the tree does not fill with husks.
                if let Some(dir) = e.path.parent() {
                    let _ = std::fs::remove_dir(dir);
                }
            }
        }
        Ok(removed)
    }

    fn entries(&self) -> Result<Vec<Entry>> {
        let mut out = Vec::new();
        if !self.root.exists() {
            return Ok(out);
        }
        for entry in walkdir::WalkDir::new(&self.root)
            .min_depth(3)
            .max_depth(3)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            out.push(Entry {
                path: entry.into_path(),
                size: meta.len(),
                modified: meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            });
        }
        Ok(out)
    }
}

struct Entry {
    path: PathBuf,
    size: u64,
    modified: std::time::SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32) -> DecodedImage {
        DecodedImage {
            width: w,
            height: h,
            rgba: vec![128; (w * h * 4) as usize],
        }
    }

    fn fp(n: u64) -> Fingerprint {
        Fingerprint {
            hash: n.wrapping_mul(0x9E37_79B9_7F4A_7C15),
            size: n,
        }
    }

    #[test]
    fn paths_are_sharded_by_fingerprint() {
        let cache = ThumbCache::new(PathBuf::from("/c"));
        let p = cache.path_for(fp(1), ThumbLevel::Small);
        let hex = fp(1).to_hex();
        assert_eq!(
            p,
            PathBuf::from("/c")
                .join(&hex[..2])
                .join(&hex)
                .join("256.jpg")
        );
        assert_ne!(cache.path_for(fp(1), ThumbLevel::Medium), p);
    }

    #[test]
    fn write_then_exists_and_readable_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbCache::new(dir.path().join("thumbs"));
        assert!(!cache.exists(fp(1), ThumbLevel::Small));
        let path = cache.write(fp(1), ThumbLevel::Small, &img(8, 6)).unwrap();
        assert!(cache.exists(fp(1), ThumbLevel::Small));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        assert!(
            std::fs::read_dir(path.parent().unwrap())
                .unwrap()
                .all(|e| !e.unwrap().file_name().to_string_lossy().contains("tmp"))
        );
    }

    #[test]
    fn remove_deletes_all_levels_and_tolerates_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbCache::new(dir.path().to_path_buf());
        cache.write(fp(2), ThumbLevel::Small, &img(4, 4)).unwrap();
        cache.write(fp(2), ThumbLevel::Medium, &img(4, 4)).unwrap();
        cache.remove(fp(2)).unwrap();
        assert!(!cache.exists(fp(2), ThumbLevel::Small));
        assert!(!cache.exists(fp(2), ThumbLevel::Medium));
        cache.remove(fp(2)).unwrap();
    }

    #[test]
    fn eviction_removes_oldest_first_until_under_cap() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbCache::new(dir.path().to_path_buf());
        assert_eq!(cache.size_on_disk().unwrap(), 0);
        let mut paths = Vec::new();
        for n in 1..=4 {
            let p = cache.write(fp(n), ThumbLevel::Small, &img(32, 32)).unwrap();
            // Force distinct, increasing mtimes regardless of filesystem resolution.
            let t = std::time::SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(1_000_000 + n * 10);
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(t)
                .unwrap();
            paths.push(p);
        }
        let per = std::fs::metadata(&paths[0]).unwrap().len();
        let total = cache.size_on_disk().unwrap();
        assert_eq!(total, per * 4);
        assert_eq!(cache.evict_to(total).unwrap(), 0);
        let removed = cache.evict_to(per * 2).unwrap();
        assert_eq!(removed, 2);
        assert!(!paths[0].exists() && !paths[1].exists());
        assert!(paths[2].exists() && paths[3].exists());
        assert!(cache.size_on_disk().unwrap() <= per * 2);
    }
}
