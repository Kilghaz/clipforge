//! Whether the OS can play what we export. Windows plays HEVC (Photos,
//! Media Player, Explorer thumbnails) only with Microsoft's "HEVC Video
//! Extensions" package from the Store; macOS always can.

/// Store page of the free "HEVC Video Extensions from Device Manufacturer".
pub const HEVC_STORE_URI: &str = "ms-windows-store://pdp/?ProductId=9N4WGH0Z6VHQ";

/// Package-family prefix shared by the free and the paid HEVC extension.
const HEVC_PACKAGE: &str = "Microsoft.HEVCVideoExtension";

/// `Some(true)` if the OS plays HEVC, `Some(false)` if an extension is
/// missing, `None` if it cannot be told. Spawns `reg.exe` on Windows (well
/// under a second); call it off the UI thread.
#[must_use]
pub fn hevc_playback() -> Option<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| lists_hevc_package(&String::from_utf8_lossy(&out.stdout)))
    }
    #[cfg(not(windows))]
    {
        Some(true)
    }
}

/// Whether a `reg query` listing of installed packages names the HEVC
/// extension (one package full name per line, e.g.
/// `...\Packages\Microsoft.HEVCVideoExtension_2.1.1161.0_x64__8wekyb3d8bbwe`).
#[must_use]
pub fn lists_hevc_package(listing: &str) -> bool {
    listing.lines().any(|line| {
        line.rsplit('\\')
            .next()
            .is_some_and(|name| name.trim().starts_with(HEVC_PACKAGE))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = r"HKEY_CURRENT_USER\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";

    #[test]
    fn finds_the_free_and_the_paid_extension() {
        let with = format!(
            "\r\n{ROOT}\\Microsoft.Photos_2024.11070.15005.0_x64__8wekyb3d8bbwe\r\n{ROOT}\\Microsoft.HEVCVideoExtension_2.1.1161.0_x64__8wekyb3d8bbwe\r\n"
        );
        assert!(lists_hevc_package(&with));
        let paid = format!("{ROOT}\\Microsoft.HEVCVideoExtensions_2.0.0.0_x64__8wekyb3d8bbwe");
        assert!(lists_hevc_package(&paid));
    }

    #[test]
    fn other_packages_and_empty_output_mean_missing() {
        let without = format!(
            "{ROOT}\\Microsoft.HEIFImageExtension_1.2.3.0_x64__8wekyb3d8bbwe\r\n{ROOT}\\Microsoft.VP9VideoExtensions_1.0.0.0_x64__8wekyb3d8bbwe"
        );
        assert!(!lists_hevc_package(&without));
        assert!(!lists_hevc_package(""));
        // The name must be the key itself, not somewhere in the path.
        assert!(!lists_hevc_package(r"C:\Microsoft.HEVCVideoExtension\x"));
    }

    #[test]
    fn non_windows_always_plays_hevc() {
        if !cfg!(windows) {
            assert_eq!(hevc_playback(), Some(true));
        }
    }
}
