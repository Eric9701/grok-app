//! Atlas white-label: remote App / CLI update checks stay off.
//!
//! First-run CLI install from the enterprise mirror is unchanged
//! (`cli_install`). This flag only stops phone-home version probes
//! (`app_check_update`, `cli_update_check`, Tauri updater, boot offer).

/// When `false`, Host does not query GitHub or run `atlas update --check`.
pub const REMOTE_UPDATES_ENABLED: bool = false;

pub fn disabled_message() -> &'static str {
    "Remote updates are disabled on this Atlas build"
}
