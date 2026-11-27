//! Locate the headless `tabularis` binary.
//!
//! Order: `ATLAS_TABULARIS_BIN`, a saved settings path, the packaged resource,
//! a sibling checkout build, then `PATH`.

use std::path::{Path, PathBuf};

pub fn binary_file_name() -> &'static str {
    if cfg!(windows) {
        "tabularis.exe"
    } else {
        "tabularis"
    }
}

pub fn resolve(explicit: Option<&str>, resource_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = env_path("ATLAS_TABULARIS_BIN") {
        return Some(path);
    }
    if let Some(path) = explicit.and_then(existing_file) {
        return Some(PathBuf::from(path));
    }
    if let Some(dir) = resource_dir {
        let packaged = dir.join("tabularis").join(binary_file_name());
        if packaged.is_file() {
            return Some(packaged);
        }
    }
    if let Some(path) = packaged_beside_executable() {
        return Some(path);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sibling = manifest
        .join("../../tabularis/src-tauri/target/release")
        .join(binary_file_name());
    if sibling.is_file() {
        return Some(sibling);
    }
    which_on_path(binary_file_name())
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var(key).ok().and_then(|value| existing_file(value.trim()).map(PathBuf::from))
}

fn existing_file(path: &str) -> Option<&str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    Path::new(trimmed).is_file().then_some(trimmed)
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}

fn packaged_beside_executable() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = binary_file_name();
    let mut candidates = vec![
        dir.join("tabularis").join(name),
        dir.join("resources").join("tabularis").join(name),
    ];
    if let Some(contents) = dir.parent() {
        candidates.push(contents.join("Resources").join("tabularis").join(name));
    }
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_name_matches_the_platform() {
        let name = binary_file_name();
        if cfg!(windows) {
            assert!(name.ends_with(".exe"));
        } else {
            assert_eq!(name, "tabularis");
        }
    }
}
