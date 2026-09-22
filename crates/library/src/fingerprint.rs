//! Cheap content fingerprint used to recognise a file after it moved.
//!
//! Hashing whole multi-gigabyte videos on import is too slow, so the
//! fingerprint covers the file size plus the first and last 1 MiB. That is
//! enough to tell two different files apart in practice and is cheap even
//! for files on slow or cloud-backed storage.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::Xxh3;

/// Bytes hashed from each end of the file.
pub const WINDOW: u64 = 1024 * 1024;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fingerprint {
    pub size: u64,
    pub hash: u64,
}

impl Fingerprint {
    /// Computes the fingerprint of a seekable stream. Files up to `2 * WINDOW`
    /// are hashed in full; larger files contribute their head and tail.
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> io::Result<Fingerprint> {
        let size = reader.seek(SeekFrom::End(0))?;
        let mut hasher = Xxh3::new();
        hasher.update(&size.to_le_bytes());

        if size <= 2 * WINDOW {
            reader.seek(SeekFrom::Start(0))?;
            io::copy(reader, &mut HashWriter(&mut hasher))?;
        } else {
            reader.seek(SeekFrom::Start(0))?;
            io::copy(
                &mut reader.by_ref().take(WINDOW),
                &mut HashWriter(&mut hasher),
            )?;
            reader.seek(SeekFrom::Start(size - WINDOW))?;
            io::copy(
                &mut reader.by_ref().take(WINDOW),
                &mut HashWriter(&mut hasher),
            )?;
        }
        Ok(Fingerprint {
            size,
            hash: hasher.digest(),
        })
    }

    /// Opens and fingerprints a file on disk.
    pub fn from_path(path: &std::path::Path) -> io::Result<Fingerprint> {
        let mut file = std::fs::File::open(path)?;
        Self::from_reader(&mut file)
    }

    /// Stable textual form, usable as a cache directory name.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{:016x}-{:x}", self.hash, self.size)
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

struct HashWriter<'a>(&'a mut Xxh3);

impl io::Write for HashWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn fp(bytes: &[u8]) -> Fingerprint {
        Fingerprint::from_reader(&mut Cursor::new(bytes)).unwrap()
    }

    #[test]
    fn identical_content_gives_identical_fingerprint() {
        assert_eq!(fp(b"hello"), fp(b"hello"));
        assert_eq!(fp(b"hello").size, 5);
    }

    #[test]
    fn small_files_are_sensitive_to_every_byte() {
        assert_ne!(fp(b"hello"), fp(b"hellp"));
        assert_ne!(fp(b""), fp(b"a"));
    }

    #[test]
    fn same_size_different_content_differs() {
        assert_ne!(fp(&[0u8; 100]), fp(&[1u8; 100]));
    }

    #[test]
    fn large_files_hash_head_and_tail_only() {
        let len = (3 * WINDOW) as usize;
        let mut a = vec![0u8; len];
        let mut b = vec![0u8; len];
        // Change in the middle: invisible by design.
        b[len / 2] = 1;
        assert_eq!(fp(&a), fp(&b));
        // Change in the head: visible.
        b[len / 2] = 0;
        b[10] = 1;
        assert_ne!(fp(&a), fp(&b));
        // Change in the tail: visible.
        b[10] = 0;
        b[len - 10] = 1;
        assert_ne!(fp(&a), fp(&b));
        // Sanity: back to equal.
        b[len - 10] = 0;
        a[0] = 0;
        assert_eq!(fp(&a), fp(&b));
    }

    #[test]
    fn file_and_cursor_agree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.bin");
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &data).unwrap();
        assert_eq!(Fingerprint::from_path(&path).unwrap(), fp(&data));
    }

    #[test]
    fn hex_form_is_filesystem_safe_and_stable() {
        let h = fp(b"abc").to_hex();
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        assert_eq!(h, fp(b"abc").to_string());
    }
}
