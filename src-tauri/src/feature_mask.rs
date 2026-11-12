//! Desktop feature mask from atlas-server.
//!
//! `GET {login_base}/v1/feature-mask` with the CLI bearer token.
//! Any failure, including a missing token, is mask 0. The token never leaves the host.

use serde_json::Value;

/// Bits from the right: 1 fleet, 2 wiki, 4 remote, 8 agent CLI.
#[tauri::command]
pub async fn feature_mask_get() -> i64 {
    fetch_feature_mask().await
}

async fn fetch_feature_mask() -> i64 {
    let Some(token) = crate::account::read_access_token_for_mask() else {
        return 0;
    };
    let url = feature_mask_url();
    let client = match crate::proxy::apply_to_reqwest(
        reqwest::Client::builder().timeout(std::time::Duration::from_secs(8)),
    )
    .build()
    {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let response = match client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return 0,
    };
    if !response.status().is_success() {
        return 0;
    }
    let body = match response.text().await {
        Ok(t) => t,
        Err(_) => return 0,
    };
    mask_from_body(&body)
}

fn feature_mask_url() -> String {
    let snap = crate::atlas_endpoints::read_atlas_endpoints();
    let base = crate::atlas_endpoints::endpoint_value(&snap.entries, "login_base_url")
        .unwrap_or_else(|| crate::account_private::atlas_auth_base());
    format!("{}/v1/feature-mask", base.trim().trim_end_matches('/'))
}

/// Accept only a JSON integer in `0..=65535`. Anything else is 0.
pub fn mask_from_body(body: &str) -> i64 {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return 0;
    };
    let Some(n) = v.get("featureMask").and_then(Value::as_i64) else {
        return 0;
    };
    if (0..=65535).contains(&n) { n } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_is_fleet_and_wiki() {
        assert_eq!(mask_from_body(r#"{"featureMask":3}"#), 3);
    }

    #[test]
    fn bad_payloads_are_zero() {
        assert_eq!(mask_from_body(""), 0);
        assert_eq!(mask_from_body(r#"{"featureMask":"3"}"#), 0);
        assert_eq!(mask_from_body(r#"{"featureMask":3.5}"#), 0);
        assert_eq!(mask_from_body(r#"{"featureMask":-1}"#), 0);
        assert_eq!(mask_from_body(r#"{"featureMask":65536}"#), 0);
        assert_eq!(mask_from_body("{}"), 0);
    }
}
