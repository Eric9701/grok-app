//! Approve an in-flight Atlas CLI device login against the private atlas-server.
//!
//! The CLI still polls `/oauth2/token` and writes `~/.atlas/auth.json`. This
//! module only performs the human step the device page does: web login, then
//! submit the account machine code for the CLI `user_code`.

use serde::Serialize;

const DEVICE_EVENT: &str = "account://device-code";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeEvent {
    pub user_code: String,
    pub verification_uri: Option<String>,
}

pub fn device_event_name() -> &'static str {
    DEVICE_EVENT
}

/// `http://host:22255/atlas/cli` → `http://host:22255/atlas`.
pub fn atlas_auth_base_from_mirror(mirror: &str) -> String {
    let trimmed = mirror.trim().trim_end_matches('/');
    trimmed.strip_suffix("/cli").unwrap_or(trimmed).to_string()
}

pub fn atlas_auth_base() -> String {
    atlas_auth_base_from_mirror(&crate::cli_install::resolve_cli_mirror())
}

/// Pull a CLI user code out of a login stdout line or verification URL.
pub fn user_code_from_login_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(idx) = trimmed.find("user_code=") {
        let rest = &trimmed[idx + "user_code=".len()..];
        let code = rest
            .split(['&', ' ', '"', '\'', '>', '<'])
            .next()
            .unwrap_or("")
            .trim();
        if !code.is_empty() {
            return Some(code.to_string());
        }
    }
    for prefix in ["user_code:", "User code:", "code:", "Code:"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let code = rest.trim().trim_matches(|c: char| c == '"' || c == '\'');
            if !code.is_empty() && code.len() <= 64 {
                return Some(code.to_string());
            }
        }
    }
    // `atlas login --device-auth` prints the code alone, indented, on stderr:
    // `  XXXX-XXXX`.
    if looks_like_user_code(trimmed) {
        return Some(trimmed.to_string());
    }
    None
}

fn looks_like_user_code(raw: &str) -> bool {
    let mut parts = raw.split('-');
    let (Some(left), Some(right)) = (parts.next(), parts.next()) else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    let chunk = |s: &str| {
        s.len() == 4
            && s.chars().all(|c| {
                (c.is_ascii_uppercase() && c != 'I' && c != 'O')
                    || (c.is_ascii_digit() && c != '0' && c != '1')
            })
    };
    chunk(left) && chunk(right)
}

pub fn verification_uri_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    None
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn encode_component(raw: &str) -> String {
    let mut out = String::new();
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn session_cookie(headers: &reqwest::header::HeaderMap) -> Option<String> {
    for value in headers.get_all(reqwest::header::SET_COOKIE) {
        let Ok(text) = value.to_str() else {
            continue;
        };
        let pair = text.split(';').next().unwrap_or("").trim();
        if let Some(token) = pair.strip_prefix("atlas_session=") {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

/// Interpret the device-page HTML. Success is the approved sentence.
pub fn device_approval_result(body: &str) -> Result<(), String> {
    if body.contains("Approved. You can return") {
        return Ok(());
    }
    for marker in [
        "machine code does not match your account",
        "your account has no machine code",
        "invalid email or password",
        "Authorization denied.",
        "unknown user code",
        "expired",
    ] {
        if body.contains(marker) {
            return Err(marker.to_string());
        }
    }
    Err("private approval did not succeed".into())
}

/// Log into atlas-server and approve the pending CLI user code with the
/// account machine code. Does not write `auth.json`; the CLI poll does that.
pub async fn approve_machine_login(
    user_code: &str,
    email: &str,
    password: &str,
    machine_code: &str,
) -> Result<(), String> {
    let user_code = user_code.trim();
    let email = email.trim();
    let machine_code = machine_code.trim();
    if user_code.is_empty() {
        return Err("CLI user code is not ready yet".into());
    }
    if email.is_empty() || password.is_empty() {
        return Err("email and password are required".into());
    }
    if machine_code.is_empty() {
        return Err("machine code is required".into());
    }

    let base = atlas_auth_base();
    let client = crate::proxy::apply_to_reqwest(
        reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)),
    )
    .build()
    .map_err(|e| e.to_string())?;

    let login = client
        .post(format!("{base}/api/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| format!("private login request failed: {e}"))?;
    if !login.status().is_success() {
        return Err("invalid email or password".into());
    }
    let session = session_cookie(login.headers())
        .ok_or_else(|| "private login did not return a session".to_string())?;

    let body = form_encode(&[
        ("user_code", user_code),
        ("machine_code", machine_code),
        ("action", "approve"),
    ]);
    let approved = client
        .post(format!("{base}/oauth2/device"))
        .header(reqwest::header::COOKIE, format!("atlas_session={session}"))
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_err(|e| format!("machine-code approval failed: {e}"))?;
    let text = approved
        .text()
        .await
        .map_err(|e| format!("machine-code approval response: {e}"))?;
    device_approval_result(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_strips_cli_suffix() {
        assert_eq!(
            atlas_auth_base_from_mirror("http://10.218.220.237:22255/atlas/cli"),
            "http://10.218.220.237:22255/atlas"
        );
        assert_eq!(
            atlas_auth_base_from_mirror("http://127.0.0.1:22255/atlas/cli/"),
            "http://127.0.0.1:22255/atlas"
        );
    }

    #[test]
    fn reads_user_code_from_url_and_label() {
        assert_eq!(
            user_code_from_login_line(
                "http://10.218.220.237:22255/atlas/oauth2/device?user_code=ABCD-EFGH"
            )
            .as_deref(),
            Some("ABCD-EFGH")
        );
        assert_eq!(
            user_code_from_login_line("user_code: WXYZ-2345").as_deref(),
            Some("WXYZ-2345")
        );
        assert_eq!(
            user_code_from_login_line("  ABCD-EFGH").as_deref(),
            Some("ABCD-EFGH")
        );
        assert!(user_code_from_login_line("Waiting for authorization...").is_none());
    }

    #[test]
    fn approval_html() {
        assert!(device_approval_result(
            "<p class=\"ok\">Approved. You can return to the Atlas CLI.</p>"
        )
        .is_ok());
        let err =
            device_approval_result("<p class=\"err\">machine code does not match your account</p>")
                .unwrap_err();
        assert!(err.contains("does not match"));
    }
}
