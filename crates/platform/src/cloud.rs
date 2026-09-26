//! Detecting cloud placeholders: files whose bytes live in iCloud Drive,
//! iCloud for Windows, OneDrive or similar and would be downloaded on
//! first read. Nothing here reads file contents.

use std::path::{Path, PathBuf};

/// Result of a placeholder check.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CloudStatus {
    /// Bytes are on disk.
    Local,
    /// Reading would trigger a download.
    Placeholder,
}

/// What an iCloud Drive `.name.icloud` stub stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcloudStub {
    /// Path of the stub file itself.
    pub stub: PathBuf,
    /// Path the real file will have once downloaded.
    pub target: PathBuf,
    /// Size announced in the stub, if present.
    pub size: Option<u64>,
}

/// Recognises macOS iCloud Drive stubs (`.IMG_0001.HEIC.icloud`). Pure
/// path logic; works on every OS so tests can run anywhere.
#[must_use]
pub fn icloud_stub(path: &Path) -> Option<IcloudStub> {
    let name = path.file_name()?.to_str()?;
    let inner = name.strip_prefix('.')?.strip_suffix(".icloud")?;
    if inner.is_empty() {
        return None;
    }
    let target = path.with_file_name(inner);
    Some(IcloudStub {
        stub: path.to_path_buf(),
        target,
        size: stub_size(path),
    })
}

/// The stub is a property list with an `NSURLFileSizeKey` integer. We only
/// handle the XML form; binary plists just yield `None`.
fn stub_size(path: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(path).ok()?;
    let idx = text.find("NSURLFileSizeKey")?;
    let rest = &text[idx..];
    let start = rest.find("<integer>")? + "<integer>".len();
    let end = rest[start..].find("</integer>")? + start;
    rest[start..end].trim().parse().ok()
}

/// Checks the file's own attributes for placeholder flags.
///
/// - macOS: `SF_DATALESS` in `st_flags` (File Provider / iCloud Drive).
/// - Windows: `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`, `_RECALL_ON_OPEN` or
///   `_OFFLINE` (Cloud Files API: iCloud for Windows, OneDrive, Dropbox).
///
/// Unknown providers therefore look local, which is the safe default.
#[must_use]
pub fn cloud_status(path: &Path) -> CloudStatus {
    if icloud_stub(path).is_some() {
        return CloudStatus::Placeholder;
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) => cloud_status_from_metadata(&meta),
        Err(_) => CloudStatus::Local,
    }
}

#[cfg(target_os = "macos")]
fn cloud_status_from_metadata(meta: &std::fs::Metadata) -> CloudStatus {
    use std::os::macos::fs::MetadataExt;
    const SF_DATALESS: u32 = 0x4000_0000;
    if meta.st_flags() & SF_DATALESS != 0 {
        CloudStatus::Placeholder
    } else {
        CloudStatus::Local
    }
}

#[cfg(windows)]
fn cloud_status_from_metadata(meta: &std::fs::Metadata) -> CloudStatus {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    let attrs = meta.file_attributes();
    if attrs
        & (FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
        != 0
    {
        CloudStatus::Placeholder
    } else {
        CloudStatus::Local
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn cloud_status_from_metadata(_meta: &std::fs::Metadata) -> CloudStatus {
    CloudStatus::Local
}

/// Chunk size for reading a placeholder through.
const HYDRATE_CHUNK: usize = 1 << 20;
/// How long to wait for iCloud to deliver a stubbed file.
const ICLOUD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Brings a cloud file's bytes onto the disk (ADR-0007: only as an
/// explicit, visible job). `path` is the file's real path (for a macOS
/// `.name.icloud` stub, the name without the stub).
///
/// - Cloud Files (Windows) and File Provider (macOS) placeholders are read
///   through once; the OS downloads what is read.
/// - A macOS iCloud stub is requested with `brctl download` and awaited.
///
/// `progress(done, total)` reports bytes; `keep_going` is polled between
/// chunks and returning `false` stops with `ErrorKind::Interrupted`.
pub fn hydrate(
    path: &Path,
    progress: &mut dyn FnMut(u64, u64),
    keep_going: &dyn Fn() -> bool,
) -> std::io::Result<()> {
    let stub = stub_for(path);
    if !path.exists() && stub.as_ref().is_some_and(|s| s.exists()) {
        request_icloud_download(path)?;
        let size = stub.as_deref().and_then(stub_size).unwrap_or(0);
        let started = std::time::Instant::now();
        while !(path.exists() && cloud_status(path) == CloudStatus::Local) {
            if !keep_going() {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            if started.elapsed() > ICLOUD_TIMEOUT {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
            progress(0, size);
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        progress(size, size);
        return Ok(());
    }
    read_through(path, progress, keep_going)
}

fn read_through(
    path: &Path,
    progress: &mut dyn FnMut(u64, u64),
    keep_going: &dyn Fn() -> bool,
) -> std::io::Result<()> {
    use std::io::Read;
    let total = std::fs::metadata(path)?.len();
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; HYDRATE_CHUNK];
    let mut done = 0u64;
    progress(0, total);
    loop {
        if !keep_going() {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        done += n as u64;
        progress(done, total);
    }
}

/// The `.name.icloud` stub that stands for `path`, if the name allows one.
fn stub_for(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    Some(path.with_file_name(format!(".{name}.icloud")))
}

fn request_icloud_download(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("brctl")
            .arg("download")
            .arg(path)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "brctl download failed: {status}"
            )))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_icloud_stubs_by_name() {
        let stub = icloud_stub(Path::new("/x/.IMG_0001.HEIC.icloud")).unwrap();
        assert_eq!(stub.target, PathBuf::from("/x/IMG_0001.HEIC"));
        assert_eq!(stub.size, None, "unreadable stub has no size");
        assert!(icloud_stub(Path::new("/x/IMG_0001.HEIC")).is_none());
        assert!(icloud_stub(Path::new("/x/.hidden.icloud.jpg")).is_none());
        assert!(icloud_stub(Path::new("/x/..icloud")).is_none());
        assert!(
            icloud_stub(Path::new("/x/photo.icloud")).is_none(),
            "needs the leading dot"
        );
    }

    #[test]
    fn reads_size_from_xml_stub() {
        let dir = tempfile::tempdir().unwrap();
        let stub = dir.path().join(".a.jpg.icloud");
        std::fs::write(
            &stub,
            "<plist><dict><key>NSURLFileSizeKey</key><integer>12345</integer></dict></plist>",
        )
        .unwrap();
        let s = icloud_stub(&stub).unwrap();
        assert_eq!(s.size, Some(12345));
        assert_eq!(cloud_status(&stub), CloudStatus::Placeholder);
    }

    #[test]
    fn hydrate_reads_the_file_through_with_progress() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("big.mov");
        std::fs::write(&f, vec![7u8; HYDRATE_CHUNK * 2 + 10]).unwrap();
        let mut seen = Vec::new();
        hydrate(&f, &mut |d, t| seen.push((d, t)), &|| true).unwrap();
        let total = (HYDRATE_CHUNK * 2 + 10) as u64;
        assert_eq!(seen.first(), Some(&(0, total)));
        assert_eq!(seen.last(), Some(&(total, total)));
        assert!(seen.len() >= 3, "one report per chunk");
    }

    #[test]
    fn hydrate_stops_when_cancelled_and_fails_for_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.jpg");
        std::fs::write(&f, b"x").unwrap();
        let err = hydrate(&f, &mut |_, _| {}, &|| false).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
        assert!(hydrate(&dir.path().join("gone.jpg"), &mut |_, _| {}, &|| true).is_err());
    }

    #[test]
    fn hydrate_asks_icloud_for_a_stubbed_file() {
        // Only the stub exists: the real file must be requested, which is
        // unsupported off macOS and cancellable on it.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("IMG_1.HEIC");
        std::fs::write(dir.path().join(".IMG_1.HEIC.icloud"), b"<plist/>").unwrap();
        let err = hydrate(&target, &mut |_, _| {}, &|| false).unwrap_err();
        if cfg!(target_os = "macos") {
            // brctl rejects a path outside iCloud Drive, or we cancel.
            assert!(
                matches!(
                    err.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::Other
                ),
                "{err}"
            );
        } else {
            assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
        }
    }

    #[test]
    fn ordinary_files_and_missing_files_are_local() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.jpg");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(cloud_status(&f), CloudStatus::Local);
        assert_eq!(
            cloud_status(&dir.path().join("missing.jpg")),
            CloudStatus::Local
        );
    }
}
