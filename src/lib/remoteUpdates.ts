/**
 * Atlas white-label: no GitHub / Tauri updater / `atlas update --check`.
 * First-run CLI install from the enterprise mirror is unchanged.
 */
export const REMOTE_UPDATES_ENABLED = false;

/**
 * Desktop self-update from the enterprise atlas-server (Host `self_update_*`).
 * Independent of the GitHub / CLI checks above. Mirrors
 * `src-tauri/src/remote_updates.rs`.
 */
export const DESKTOP_SELF_UPDATE_ENABLED = true;

/** Self-update replaces the GitHub / Tauri plugin path on this build. */
export const SELF_UPDATE_MODE = DESKTOP_SELF_UPDATE_ENABLED && !REMOTE_UPDATES_ENABLED;

/** Any app update path is active (background checks, About actions). */
export const APP_UPDATES_ACTIVE = REMOTE_UPDATES_ENABLED || DESKTOP_SELF_UPDATE_ENABLED;
