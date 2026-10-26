//! AtlasBuddy desktop self-update from atlas-server — no signing key.
//!
//! Feed: the CLI mirror `…/atlas/cli` becomes `…/atlas/desktop/stable/update.json`.
//! Integrity is the feed's SHA-256 + size; trust rests on atlas-server and the
//! transport (HTTPS recommended). Payload URLs must share the feed's origin.
//!
//! Flow: check → background download + verify → user confirms → install.
//! Nothing installs or restarts without `self_update_install`.

mod apply;
mod download;
mod feed;
mod pending;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, State};
use tracing::{info, warn};

use crate::mirror::MirrorHost;
use crate::remote_im::RemoteImState;
use crate::session_manager::SessionManager;
use crate::voice_host::VoiceHost;

use feed::FeedPlatform;
use pending::BootOutcome;

/// Same version failing to install this many times stops the auto prompt.
const MAX_AUTO_ATTEMPTS: u32 = 2;

struct Staged {
    version: String,
    platform: FeedPlatform,
    file_name: String,
    path: Option<PathBuf>,
}

static STAGED: Mutex<Option<Staged>> = Mutex::new(None);
static LAST_FAILED: Mutex<Option<FailedUpdate>> = Mutex::new(None);
static OP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn enabled() -> bool {
    crate::remote_updates::DESKTOP_SELF_UPDATE_ENABLED
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedUpdate {
    pub version: String,
    pub attempts: u32,
    /// First launch after the failure — show the notice once.
    pub notify: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfUpdateStatus {
    pub enabled: bool,
    pub current_version: String,
    pub feed_url: String,
    /// Why this copy can only update via the manual installer.
    pub blocker: Option<String>,
    pub last_failed: Option<FailedUpdate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfUpdateCheck {
    pub current_version: String,
    pub update_available: bool,
    pub version: String,
    pub notes: String,
    pub can_auto_install: bool,
    pub blocker: Option<String>,
    pub installer_url: Option<String>,
    /// Failed installs of this exact version so far.
    pub failed_attempts: u32,
    /// `false` after repeated failures: keep About entry, skip the popup.
    pub auto_prompt: bool,
}

/// Run once in setup: classify the previous install and sweep old payloads.
pub fn run_on_startup() {
    if !enabled() {
        return;
    }
    let (outcome, keep) = pending::evaluate(pending::read(), current_version(), chrono::Utc::now());
    match &outcome {
        BootOutcome::Nothing => download::sweep_stale(None),
        BootOutcome::Succeeded { to } => {
            info!(target: "grok_app::self_update", version = %to, "update installed");
            pending::clear();
            download::sweep_stale(None);
        }
        BootOutcome::Expired => {
            pending::clear();
            download::sweep_stale(None);
        }
        BootOutcome::Failed {
            version,
            attempts,
            notify,
        } => {
            warn!(target: "grok_app::self_update", %version, attempts, "previous update did not install");
            if let Some(p) = &keep {
                let _ = pending::write(p);
            }
            // Keep the verified payload so a retry does not re-download.
            download::sweep_stale(Some(version));
            *LAST_FAILED.lock() = Some(FailedUpdate {
                version: version.clone(),
                attempts: *attempts,
                notify: *notify,
            });
        }
    }
}

fn failed_attempts_for(version: &str) -> u32 {
    LAST_FAILED
        .lock()
        .as_ref()
        .filter(|f| f.version == version)
        .map_or(0, |f| f.attempts)
}

#[tauri::command]
pub fn self_update_status() -> SelfUpdateStatus {
    SelfUpdateStatus {
        enabled: enabled(),
        current_version: current_version().to_string(),
        feed_url: feed::resolve_feed_url().unwrap_or_default(),
        blocker: apply::blocker().map(str::to_string),
        last_failed: LAST_FAILED.lock().clone(),
    }
}

/// Mark the failure notice as shown for this run.
#[tauri::command]
pub fn self_update_ack_failure() {
    if let Some(f) = LAST_FAILED.lock().as_mut() {
        f.notify = false;
    }
}

#[tauri::command]
pub async fn self_update_check() -> Result<SelfUpdateCheck, String> {
    if !enabled() {
        return Err("self_update_disabled".into());
    }
    let _guard = OP_LOCK.try_lock().map_err(|_| "busy".to_string())?;
    let feed_url = feed::resolve_feed_url().ok_or("invalid_mirror")?;
    let f = download::fetch_feed(&feed_url).await?;
    let current = current_version();
    let mut result = SelfUpdateCheck {
        current_version: current.to_string(),
        update_available: false,
        version: f.version.clone(),
        notes: f.notes.clone(),
        can_auto_install: false,
        blocker: None,
        installer_url: None,
        failed_attempts: 0,
        auto_prompt: false,
    };
    if !crate::app_update::is_remote_newer(current, &f.version) {
        *STAGED.lock() = None;
        return Ok(result);
    }
    result.update_available = true;

    let Some((key, kind)) = apply::current_feed_key() else {
        result.blocker = Some("unsupported_platform".into());
        return Ok(result);
    };
    result.installer_url = f
        .installers
        .get(key)
        .filter(|u| feed::is_allowed_payload_url(&feed_url, u))
        .cloned();

    let payload = f.platforms.get(key).filter(|p| {
        p.kind == kind
            && p.size > 0
            && feed::is_valid_sha256(&p.sha256)
            && feed::is_allowed_payload_url(&feed_url, &p.url)
    });
    let Some(payload) = payload else {
        result.blocker = Some("no_payload".into());
        return Ok(result);
    };
    let file_name = feed::payload_file_name(&payload.url).ok_or("no_payload")?;

    result.blocker = apply::blocker().map(str::to_string);
    result.can_auto_install = result.blocker.is_none();
    result.failed_attempts = failed_attempts_for(&f.version);
    result.auto_prompt = result.can_auto_install && result.failed_attempts < MAX_AUTO_ATTEMPTS;

    let mut staged = STAGED.lock();
    let same = staged
        .as_ref()
        .is_some_and(|s| s.version == f.version && s.platform == *payload);
    if !same {
        *staged = Some(Staged {
            version: f.version.clone(),
            platform: payload.clone(),
            file_name,
            path: None,
        });
    }
    Ok(result)
}

/// Download + verify the payload found by the last check.
#[tauri::command]
pub async fn self_update_download(app: AppHandle) -> Result<String, String> {
    let _guard = OP_LOCK.try_lock().map_err(|_| "busy".to_string())?;
    let (version, platform, file_name) = {
        let s = STAGED.lock();
        let s = s.as_ref().ok_or("not_ready")?;
        (s.version.clone(), s.platform.clone(), s.file_name.clone())
    };
    let path = download::download_payload(&app, &version, &file_name, &platform).await?;
    if let Some(s) = STAGED.lock().as_mut().filter(|s| s.version == version) {
        s.path = Some(path);
    }
    Ok(version)
}

/// User confirmed. Windows: hands off to the installer and exits.
/// macOS: swaps the bundle and returns; the caller tears down and relaunches.
#[tauri::command]
pub async fn self_update_install(
    app: AppHandle,
    mgr: State<'_, Arc<SessionManager>>,
    mirror: State<'_, Arc<MirrorHost>>,
    voice: State<'_, Arc<VoiceHost>>,
    remote_im: State<'_, Arc<RemoteImState>>,
) -> Result<(), String> {
    let _guard = OP_LOCK.try_lock().map_err(|_| "busy".to_string())?;
    if let Some(b) = apply::blocker() {
        return Err(b.to_string());
    }
    let (version, platform, path) = {
        let s = STAGED.lock();
        let s = s.as_ref().ok_or("not_ready")?;
        (
            s.version.clone(),
            s.platform.clone(),
            s.path.clone().ok_or("not_ready")?,
        )
    };
    // Re-verify: the staged file sits in a user-writable folder.
    if !download::verify_file(&path, platform.size, &platform.sha256)? {
        let _ = std::fs::remove_file(&path);
        if let Some(s) = STAGED.lock().as_mut() {
            s.path = None;
        }
        return Err("checksum: staged payload changed".into());
    }

    let marker = pending::next_attempt(
        pending::read().as_ref(),
        current_version(),
        &version,
        chrono::Utc::now(),
    );
    pending::write(&marker)?;
    info!(target: "grok_app::self_update", %version, attempt = marker.attempts, "installing update");

    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
        let install = apply::windows_install(&exe).map_err(str::to_string)?;
        let expected = apply::windows_feed_key(install).1;
        if platform.kind != expected {
            pending::clear();
            return Err("no_payload".into());
        }
        let helper = if install == apply::WindowsInstall::Portable {
            let work = download::version_dir(&version);
            Some(apply::stage_portable_swap(&path, &work, &exe)?)
        } else {
            None
        };
        crate::updater::prepare_for_app_update(app.clone(), mgr, mirror, voice, remote_im).await?;
        let launched = if let Some(script) = helper {
            apply::launch_portable_helper(&script)
        } else {
            apply::launch_nsis(&path)
        };
        if let Err(e) = launched {
            pending::clear();
            return Err(e);
        }
        app.exit(0);
        // The helper (or NSIS) relaunches. Do not return to the webview,
        // which would start the old exe again while files are being replaced.
        std::future::pending::<Result<(), String>>().await
    }
    #[cfg(target_os = "macos")]
    {
        let _ = (&app, mgr, mirror, voice, remote_im);
        let work = download::version_dir(&version);
        apply::swap_app_bundle(&path, &work)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (&app, mgr, mirror, voice, remote_im, path);
        Err("unsupported_platform".into())
    }
}
