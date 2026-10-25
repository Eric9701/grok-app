//! Process argv for chrome that is hidden for one launch.
//!
//! `--hide-remote` hides the sidebar "Connect device" entry and the
//! Settings "Remote control" section together. Absent means both stay visible.
//! The choice is not written to settings.

use serde::Serialize;

pub const HIDE_REMOTE_FLAG: &str = "--hide-remote";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchChrome {
    /// False when this process was started with `--hide-remote`.
    pub remote: bool,
}

pub fn remote_visible_from(argv: &[String]) -> bool {
    !argv.iter().any(|arg| arg == HIDE_REMOTE_FLAG)
}

pub fn remote_visible() -> bool {
    remote_visible_from(&std::env::args().collect::<Vec<_>>())
}

#[tauri::command]
pub fn launch_chrome_get() -> LaunchChrome {
    LaunchChrome {
        remote: remote_visible(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_stays_visible_without_the_flag() {
        let argv = vec!["atlasbuddy".into(), "--start-in-tray".into()];
        assert!(remote_visible_from(&argv));
    }

    #[test]
    fn hide_remote_flag_hides_the_chrome() {
        let argv = vec!["atlasbuddy".into(), "--hide-remote".into()];
        assert!(!remote_visible_from(&argv));
    }
}
