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
