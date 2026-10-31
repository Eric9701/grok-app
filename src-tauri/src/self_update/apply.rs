//! Install preconditions and the per-platform hand-off.
//!
//! - Windows installer: the NSIS `*-setup.exe` runs with the same flags
//!   tauri-plugin-updater uses (`/P /R /UPDATE`: passive UI, relaunch after
//!   install, update mode); it closes the old process itself. We exit right
//!   after launching it.
//! - Windows portable (绿色版): a zip whose folder contains `README-portable.txt`.
//!   A helper waits until this process exits, copies the new folder over the
//!   current one, then starts the exe again. The running exe cannot be
//!   overwritten in place.
//! - macOS: the new `.app` replaces the running bundle in-process (old one
//!   kept aside and restored on any failure), then the caller relaunches.

use std::path::{Path, PathBuf};

/// Why this copy cannot self-install (manual installer only), or `None`.
pub fn blocker() -> Option<&'static str> {
    if cfg!(debug_assertions) {
        return Some("debug_build");
    }
    if super::feed::platform_key().is_none() {
        return Some("unsupported_platform");
    }
    let exe = std::env::current_exe().ok()?;
    platform_blocker(&exe)
}

#[cfg(windows)]
fn platform_blocker(exe: &Path) -> Option<&'static str> {
    match windows_install(exe) {
        Ok(_) => None,
        Err(reason) => Some(reason),
    }
}

#[cfg(target_os = "macos")]
fn platform_blocker(exe: &Path) -> Option<&'static str> {
    let Some(bundle) = app_bundle_of(exe) else {
        return Some("not_installed_copy");
    };
    if bundle.to_string_lossy().contains("/AppTranslocation/") {
        return Some("translocated");
    }
    let parent = bundle.parent()?;
    if !dir_writable(parent) {
        return Some("not_writable");
    }
    None
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_blocker(_exe: &Path) -> Option<&'static str> {
    Some("unsupported_platform")
}

/// `/Applications/AtlasBuddy.app/Contents/MacOS/x` → `/Applications/AtlasBuddy.app`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn app_bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
}

#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".atlasbuddy-write-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Launch the staged NSIS installer. Caller exits the process on success.
#[cfg(windows)]
pub fn launch_nsis(installer: &Path) -> Result<(), String> {
    use windows::core::{w, HSTRING};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

    let file = HSTRING::from(installer.as_os_str());
    let params = HSTRING::from(NSIS_ARGS.join(" "));
    let rc = unsafe { ShellExecuteW(None, w!("open"), &file, &params, None, SW_SHOW) };
    // ShellExecute reports success with a value > 32.
    if rc.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("launch installer failed (code {})", rc.0 as isize))
    }
}

/// Same flags tauri-plugin-updater passes for passive NSIS updates.
#[cfg_attr(not(windows), allow(dead_code))]
pub const NSIS_ARGS: [&str; 3] = ["/P", "/R", "/UPDATE"];

/// Written by `scripts/package-windows-portable.sh`. Its presence is how a
/// green-folder copy is told apart from a loose exe or an NSIS install.
pub const PORTABLE_MARKER: &str = "README-portable.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsInstall {
    Nsis,
    Portable,
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_portable_dir(dir: &Path) -> bool {
    dir.join(PORTABLE_MARKER).is_file()
}

/// NSIS when `uninstall.exe` sits beside the exe. Portable when the green
/// folder marker is present and the folder can be replaced. Anything else
/// cannot update itself.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn windows_install(exe: &Path) -> Result<WindowsInstall, &'static str> {
    let dir = exe.parent().ok_or("not_installed_copy")?;
    if dir.join("uninstall.exe").is_file() {
        return Ok(WindowsInstall::Nsis);
    }
    if is_portable_dir(dir) {
        return if dir_writable(dir) {
            Ok(WindowsInstall::Portable)
        } else {
            Err("not_writable")
        };
    }
    Err("not_installed_copy")
}

pub fn windows_feed_key(install: WindowsInstall) -> (&'static str, &'static str) {
    match install {
        WindowsInstall::Nsis => ("windows-x86_64", "nsis"),
        WindowsInstall::Portable => ("windows-x86_64-portable", "portable_zip"),
    }
}

/// Feed slot for this running copy. A copy that cannot update itself still
/// resolves the NSIS slot, so the manual installer link stays available.
pub fn current_feed_key() -> Option<(&'static str, &'static str)> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        let exe = std::env::current_exe().ok()?;
        let install = windows_install(&exe).unwrap_or(WindowsInstall::Nsis);
        return Some(windows_feed_key(install));
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        super::feed::platform_key()
    }
}

/// Top directory inside an extracted portable zip that holds `exe_name`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn portable_payload_root(extract: &Path, exe_name: &str) -> Option<PathBuf> {
    if extract.join(exe_name).is_file() {
        return Some(extract.to_path_buf());
    }
    let mut found = None;
    for entry in std::fs::read_dir(extract).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join(exe_name).is_file() {
            if found.is_some() {
                return None;
            }
            found = Some(path);
        }
    }
    found
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    use std::fs::File;
    use std::io;

    let file = File::open(archive).map_err(|e| format!("open update archive: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("read update archive: {e}"))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("read zip entry: {e}"))?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_owned()) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&rel);
        if !out.starts_with(dest) {
            continue;
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("create dir: {e}"))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
        }
        let mut file = File::create(&out).map_err(|e| format!("create file: {e}"))?;
        io::copy(&mut entry, &mut file).map_err(|e| format!("extract file: {e}"))?;
    }
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn plain_cmd_path(path: &Path) -> Result<String, String> {
    let s = path
        .to_str()
        .ok_or("install path is not valid Unicode")?
        .to_string();
    if s.chars()
        .any(|c| matches!(c, '"' | '\n' | '\r' | '%' | '!'))
    {
        return Err("install path cannot be updated automatically".into());
    }
    Ok(s)
}

/// cmd script: wait until `pid` exits, copy `src` onto `dest`, start `exe`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn portable_apply_script(
    pid: u32,
    src: &Path,
    dest: &Path,
    exe: &Path,
) -> Result<String, String> {
    let src = plain_cmd_path(src)?;
    let dest = plain_cmd_path(dest)?;
    let exe = plain_cmd_path(exe)?;
    Ok(format!(
        "@echo off\r\n\
         setlocal EnableExtensions\r\n\
         set \"PID={pid}\"\r\n\
         set \"SRC={src}\"\r\n\
         set \"DEST={dest}\"\r\n\
         set \"EXE={exe}\"\r\n\
         set /a TRIES=0\r\n\
         :wait\r\n\
         tasklist /FI \"PID eq %PID%\" /FO CSV /NH 2>nul | findstr /C:\"%PID%\" >nul\r\n\
         if errorlevel 1 goto copy\r\n\
         set /a TRIES+=1\r\n\
         if %TRIES% GEQ 90 exit /b 1\r\n\
         ping -n 2 127.0.0.1 >nul\r\n\
         goto wait\r\n\
         :copy\r\n\
         robocopy \"%SRC%\" \"%DEST%\" /E /COPY:DAT /R:3 /W:1 /NFL /NDL /NJH /NJS /NP\r\n\
         set \"RC=%ERRORLEVEL%\"\r\n\
         if %RC% GEQ 8 exit /b %RC%\r\n\
         start \"\" \"%EXE%\"\r\n\
         exit /b 0\r\n"
    ))
}

/// Extract the zip and write `apply-portable.cmd` into `work_dir`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn stage_portable_swap(archive: &Path, work_dir: &Path, exe: &Path) -> Result<PathBuf, String> {
    use std::fs;

    let extract = work_dir.join("portable-extract");
    let _ = fs::remove_dir_all(&extract);
    fs::create_dir_all(&extract).map_err(|e| format!("create extract dir: {e}"))?;
    extract_zip(archive, &extract)?;
    let exe_name = exe
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("not_installed_copy")?;
    let root =
        portable_payload_root(&extract, exe_name).ok_or("update archive has no application")?;
    let dest = exe.parent().ok_or("not_installed_copy")?;
    let body = portable_apply_script(std::process::id(), &root, dest, exe)?;
    let script = work_dir.join("apply-portable.cmd");
    fs::write(&script, body).map_err(|e| format!("write update script: {e}"))?;
    Ok(script)
}

/// The helper is not a child of this process, so it survives `app.exit`.
#[cfg(windows)]
pub fn launch_portable_helper(script: &Path) -> Result<(), String> {
    use windows::core::{w, HSTRING};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let file = HSTRING::from("cmd.exe");
    let params = HSTRING::from(format!("/c \"{}\"", script.display()));
    let rc = unsafe { ShellExecuteW(None, w!("open"), &file, &params, None, SW_HIDE) };
    if rc.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!(
            "launch portable update failed (code {})",
            rc.0 as isize
        ))
    }
}

/// Replace the running `.app` with the one inside `archive`.
#[cfg(target_os = "macos")]
pub fn swap_app_bundle(archive: &Path, work_dir: &Path) -> Result<(), String> {
    use std::fs;
    use std::process::Command;

    let exe = std::env::current_exe().map_err(|e| format!("current exe: {e}"))?;
    let bundle = app_bundle_of(&exe).ok_or("not_installed_copy")?;
    let parent = bundle.parent().ok_or("not_installed_copy")?;
    let name = bundle
        .file_name()
        .ok_or("not_installed_copy")?
        .to_string_lossy()
        .to_string();

    let extract = work_dir.join("extract");
    let _ = fs::remove_dir_all(&extract);
    fs::create_dir_all(&extract).map_err(|e| format!("create extract dir: {e}"))?;
    let out = Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&extract)
        .output()
        .map_err(|e| format!("run tar: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "extract update: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let new_app = fs::read_dir(&extract)
        .map_err(|e| format!("read extract dir: {e}"))?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir() && p.extension().is_some_and(|e| e == "app"))
        .ok_or("update archive has no .app")?;
    if !new_app.join("Contents/MacOS").is_dir() {
        return Err("update archive .app is incomplete".into());
    }
    let _ = Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&new_app)
        .status();

    let backup = parent.join(format!(".{name}.update-old"));
    let _ = fs::remove_dir_all(&backup);
    fs::rename(&bundle, &backup).map_err(|e| format!("move current app aside: {e}"))?;

    let placed = fs::rename(&new_app, &bundle).is_ok()
        || Command::new("/usr/bin/ditto")
            .arg(&new_app)
            .arg(&bundle)
            .status()
            .is_ok_and(|s| s.success());
    if !placed {
        let _ = fs::remove_dir_all(&bundle);
        fs::rename(&backup, &bundle).map_err(|e| format!("restore previous app failed: {e}"))?;
        return Err("place new app failed; previous version restored".into());
    }
    let _ = fs::remove_dir_all(&backup);
    let _ = fs::remove_dir_all(&extract);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_app_bundle_from_exe() {
        let exe = Path::new("/Applications/AtlasBuddy.app/Contents/MacOS/grok-app");
        assert_eq!(
            app_bundle_of(exe).as_deref(),
            Some(Path::new("/Applications/AtlasBuddy.app"))
        );
        assert_eq!(app_bundle_of(Path::new("/usr/local/bin/grok-app")), None);
    }

    #[test]
    fn nsis_args_match_tauri_updater() {
        assert_eq!(NSIS_ARGS, ["/P", "/R", "/UPDATE"]);
    }

    #[test]
    fn portable_dir_is_the_green_folder_not_a_loose_exe() {
        let root =
            std::env::temp_dir().join(format!("atlasbuddy-portable-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("AtlasBuddy.exe");
        std::fs::write(&exe, b"exe").unwrap();
        assert_eq!(windows_install(&exe), Err("not_installed_copy"));

        std::fs::write(root.join(PORTABLE_MARKER), b"readme").unwrap();
        assert_eq!(windows_install(&exe), Ok(WindowsInstall::Portable));
        assert_eq!(
            windows_feed_key(WindowsInstall::Portable),
            ("windows-x86_64-portable", "portable_zip")
        );

        std::fs::write(root.join("uninstall.exe"), b"un").unwrap();
        assert_eq!(windows_install(&exe), Ok(WindowsInstall::Nsis));
        assert_eq!(
            windows_feed_key(WindowsInstall::Nsis),
            ("windows-x86_64", "nsis")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn portable_zip_root_is_the_folder_that_holds_the_exe() {
        let root = std::env::temp_dir().join(format!("atlasbuddy-zip-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let inner = root.join("AtlasBuddy_0.2.43_x64-portable");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("AtlasBuddy.exe"), b"new").unwrap();
        assert_eq!(
            portable_payload_root(&root, "AtlasBuddy.exe").as_deref(),
            Some(inner.as_path())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn portable_script_waits_for_pid_and_rejects_percent_paths() {
        let script = portable_apply_script(
            4242,
            Path::new("C:/updates/portable-extract/AtlasBuddy_0.2.43_x64-portable"),
            Path::new("D:/Apps/AtlasBuddy"),
            Path::new("D:/Apps/AtlasBuddy/AtlasBuddy.exe"),
        )
        .unwrap();
        assert!(script.contains("PID=4242"));
        assert!(script.contains("robocopy"));
        assert!(script.contains("start \"\""));
        assert!(portable_apply_script(
            1,
            Path::new("C:/bad%name"),
            Path::new("D:/Apps"),
            Path::new("D:/Apps/AtlasBuddy.exe"),
        )
        .is_err());
    }

    #[test]
    fn extract_zip_skips_path_escape_and_keeps_the_exe() {
        let root = std::env::temp_dir().join(format!("atlasbuddy-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("payload.zip");
        let extract = root.join("out");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("AtlasBuddy_0.2.43_x64-portable/AtlasBuddy.exe", opts)
                .unwrap();
            std::io::Write::write_all(&mut zip, b"new-exe").unwrap();
            zip.start_file("AtlasBuddy_0.2.43_x64-portable/README-portable.txt", opts)
                .unwrap();
            std::io::Write::write_all(&mut zip, b"readme").unwrap();
            zip.start_file("../outside.txt", opts).unwrap();
            std::io::Write::write_all(&mut zip, b"nope").unwrap();
            zip.finish().unwrap();
        }
        std::fs::create_dir_all(&extract).unwrap();
        extract_zip(&archive, &extract).unwrap();
        let exe = extract.join("AtlasBuddy_0.2.43_x64-portable/AtlasBuddy.exe");
        assert_eq!(std::fs::read(&exe).unwrap(), b"new-exe");
        assert!(!extract.join("outside.txt").exists());
        assert!(!root.join("outside.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
