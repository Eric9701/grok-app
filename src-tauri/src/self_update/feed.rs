//! Feed URL derivation, `update.json` shape, and download URL allowlist.

use std::collections::HashMap;

use serde::Deserialize;

pub const CHANNEL: &str = "stable";

/// `ATLAS_DESKTOP_UPDATE_URL` overrides the derived feed (tests / odd deploys).
pub const FEED_URL_ENV: &str = "ATLAS_DESKTOP_UPDATE_URL";

#[derive(Debug, Clone, Deserialize)]
pub struct Feed {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub platforms: HashMap<String, FeedPlatform>,
    #[serde(default)]
    pub installers: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FeedPlatform {
    pub kind: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// `…/atlas/cli` → `…/atlas/desktop/stable/update.json`.
pub fn feed_url_from_mirror(mirror: &str) -> Option<String> {
    let base = mirror.trim().trim_end_matches('/');
    let root = base.strip_suffix("/cli")?;
    if !(root.starts_with("http://") || root.starts_with("https://")) {
        return None;
    }
    Some(format!("{root}/desktop/{CHANNEL}/update.json"))
}

/// Env override first, then the resolved CLI mirror.
pub fn resolve_feed_url() -> Option<String> {
    if let Ok(v) = std::env::var(FEED_URL_ENV) {
        let t = v.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    feed_url_from_mirror(&crate::cli_install::resolve_cli_mirror())
}

/// `update.json` key for this binary, with the payload kind it must carry.
pub fn platform_key() -> Option<(&'static str, &'static str)> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(("windows-x86_64", "nsis"))
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(("darwin-aarch64", "app_tar_gz"))
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(("darwin-x86_64", "app_tar_gz"))
    } else {
        None
    }
}

/// Download URLs must share the feed's origin and directory, so a tampered
/// or misconfigured manifest cannot point the installer at another host or
/// downgrade https to http.
pub fn is_allowed_payload_url(feed_url: &str, payload_url: &str) -> bool {
    let (Ok(feed), Ok(payload)) = (url::Url::parse(feed_url), url::Url::parse(payload_url)) else {
        return false;
    };
    if payload.scheme() != feed.scheme()
        || payload.host_str() != feed.host_str()
        || payload.port_or_known_default() != feed.port_or_known_default()
        || !payload.username().is_empty()
        || payload.password().is_some()
        || payload.query().is_some()
        || payload.fragment().is_some()
    {
        return false;
    }
    let feed_path = feed.path();
    let Some(dir) = feed_path.rsplit_once('/').map(|(d, _)| d) else {
        return false;
    };
    let Some(name) = payload.path().strip_prefix(&format!("{dir}/")) else {
        return false;
    };
    is_safe_file_name(name)
}

pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Last path segment of an allowed payload URL.
pub fn payload_file_name(payload_url: &str) -> Option<String> {
    let u = url::Url::parse(payload_url).ok()?;
    let name = u.path().rsplit('/').next()?.to_string();
    is_safe_file_name(&name).then_some(name)
}

pub fn is_valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = "http://cli.example.test:22255/atlas/desktop/stable/update.json";

    #[test]
    fn derives_feed_from_cli_mirror() {
        assert_eq!(
            feed_url_from_mirror("http://cli.example.test:22255/atlas/cli/").as_deref(),
            Some(FEED)
        );
        assert_eq!(
            feed_url_from_mirror("https://atlas.example.test/cli").as_deref(),
            Some("https://atlas.example.test/desktop/stable/update.json")
        );
        assert_eq!(feed_url_from_mirror("http://cli.example.test/atlas/bin"), None);
        assert_eq!(feed_url_from_mirror("ftp://cli.example.test/cli"), None);
        assert_eq!(feed_url_from_mirror(""), None);
    }

    #[test]
    fn payload_url_must_share_origin_and_dir() {
        let ok = "http://cli.example.test:22255/atlas/desktop/stable/AtlasBuddy_1.0.0_x64-setup.exe";
        assert!(is_allowed_payload_url(FEED, ok));
        for bad in [
            "http://evil.example.test:22255/atlas/desktop/stable/a.exe",
            "https://cli.example.test:22255/atlas/desktop/stable/a.exe",
            "http://cli.example.test:9999/atlas/desktop/stable/a.exe",
            "http://cli.example.test:22255/atlas/desktop/alpha/a.exe",
            "http://cli.example.test:22255/atlas/desktop/stable/sub/a.exe",
            "http://cli.example.test:22255/atlas/desktop/stable/a.exe?x=1",
            "http://user@cli.example.test:22255/atlas/desktop/stable/a.exe",
            "http://cli.example.test:22255/atlas/desktop/stable/",
        ] {
            assert!(!is_allowed_payload_url(FEED, bad), "{bad}");
        }
        assert!(!is_allowed_payload_url(
            "https://a.example.test/desktop/stable/update.json",
            "http://a.example.test/desktop/stable/a.exe"
        ));
    }

    #[test]
    fn file_name_and_hash_checks() {
        assert_eq!(
            payload_file_name("http://h/desktop/stable/AtlasBuddy_1.0.0_aarch64.app.tar.gz").as_deref(),
            Some("AtlasBuddy_1.0.0_aarch64.app.tar.gz")
        );
        assert!(!is_safe_file_name("a..b"));
        assert!(!is_safe_file_name("a b.exe"));
        assert!(is_valid_sha256(&"a".repeat(64)));
        assert!(!is_valid_sha256(&"g".repeat(64)));
    }

    #[test]
    fn parses_feed_json() {
        let raw = r#"{"version":"1.2.3","platforms":{"windows-x86_64":{"kind":"nsis","url":"http://h/x.exe","sha256":"ab","size":3}},"installers":{}}"#;
        let f: Feed = serde_json::from_str(raw).unwrap();
        assert_eq!(f.version, "1.2.3");
        assert_eq!(f.platforms["windows-x86_64"].size, 3);
        assert!(f.notes.is_empty());
    }
}
