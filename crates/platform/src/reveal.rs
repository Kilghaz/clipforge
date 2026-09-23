//! Showing a file in Finder or Explorer.

use std::path::Path;
use std::process::Command;

/// Opens the OS file manager with `path` selected. Best effort: errors are
/// returned but nothing depends on them.
pub fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    let mut cmd = command_for(path);
    cmd.spawn().map(|_| ())
}

fn command_for(path: &Path) -> Command {
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("open");
        c.arg("-R").arg(path);
        c
    }
    #[cfg(windows)]
    {
        let mut c = Command::new("explorer");
        c.arg(format!("/select,{}", path.display()));
        c
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let mut c = Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(path));
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_selecting_command() {
        let c = command_for(Path::new("/tmp/a b.jpg"));
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|a| a.contains("a b.jpg")));
    }
}
