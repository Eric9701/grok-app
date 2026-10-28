//! Process argv for chrome that stays hidden unless this launch asks for it.
//!
//! `--show-remote` shows the sidebar "Connect device" entry and the
//! Settings "Remote control" section together. Absent means both stay hidden.
//! The choice is not written to settings.

use serde::Serialize;

pub const SHOW_REMOTE_FLAG: &str = "--show-remote";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchChrome {
    /// True only when this process was started with `--show-remote`.
    pub remote: bool,
}

pub fn remote_visible_from(argv: &[String]) -> bool {
    argv.iter().any(|arg| arg == SHOW_REMOTE_FLAG)
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
    fn remote_stays_hidden_without_the_flag() {
        let argv = vec!["atlasbuddy".into(), "--start-in-tray".into()];
        assert!(!remote_visible_from(&argv));
    }

    #[test]
    fn show_remote_flag_shows_the_chrome() {
        let argv = vec!["atlasbuddy".into(), "--show-remote".into()];
        assert!(remote_visible_from(&argv));
    }
}
