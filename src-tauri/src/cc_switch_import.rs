//! Import custom Grok Build providers from **CC Switch** (farion1231/cc-switch).
//!
//! CC Switch stores providers in SQLite (`app_type = 'grokbuild'`) under
//! `{home}/.cc-switch/` (all platforms). Optional custom root lives in Tauri
//! Store `app_paths.json` (`com.ccswitch.desktop`). Windows also has a legacy
//! `HOME/.cc-switch` fallback when Profile `.cc-switch` has no db.
//!
//! We only **read** the database. Selected custom rows are merged into the
//! shared Atlas CLI home `~/.atlas/config.toml` as `[model.<id>]` sections.
//! That merge does not change `[models].default`, does not overwrite
//! enterprise `ENC(...)` / `hidden` sections, and does not write App
//! `agent-home`. Grok Official (`category = official`) is omitted.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::process_util;
use crate::providers;

/// Test hook: when set, import/scan use `{dir}/config.toml` instead of `~/.atlas`.
const ATLAS_HOME_ENV: &str = "GROK_APP_ATLAS_HOME";
const CC_SOURCE_KEY: &str = "app_cc_switch_source";

const DB_NAME: &str = "cc-switch.db";
const STORE_APP_ID: &str = "com.ccswitch.desktop";
const STORE_FILE: &str = "app_paths.json";
const STORE_KEY_OVERRIDE: &str = "app_config_dir_override";
const ENV_DIR: &str = "GROK_APP_CC_SWITCH_DIR";
const ENV_HOME_ALT: &str = "CC_SWITCH_HOME";

// ── Public types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchScanResult {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
    pub tried_paths: Vec<String>,
    pub items: Vec<CcSwitchProviderPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchProviderPreview {
    /// CC Switch `providers.id` (UUID).
    pub source_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub is_current: bool,
    /// Suggested Atlas provider id (slug).
    pub suggested_id: String,
    pub model: String,
    pub base_url: String,
    pub api_backend: String,
    pub has_api_key: bool,
    /// Last 4 chars of key when present (no full secret).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_hint: Option<String>,
    /// `importable` | `official` | `missing_key` | `proxy_managed` | `invalid` | `exists`
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchImportRequest {
    pub source_ids: Vec<String>,
    /// `skip` | `overwrite` | `rename` (default skip)
    #[serde(default)]
    pub on_conflict: Option<String>,
    /// After import, activate this suggested/final id (optional).
    #[serde(default)]
    pub activate_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchImportResult {
    pub imported: u32,
    pub skipped: u32,
    pub failed: Vec<CcSwitchImportFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<providers::ProvidersListResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchImportFailure {
    pub source_id: String,
    pub reason: String,
}

// ── Path resolution ─────────────────────────────────────────────────────────

/// Resolve candidate dirs that may contain `cc-switch.db`, in probe order.
/// Exposed for unit tests.
pub fn candidate_cc_switch_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut push = |p: PathBuf| {
        let key = normalize_path_key(&p);
        if seen.insert(key) {
            out.push(p);
        }
    };

    // 1) Env overrides
    for env in [ENV_DIR, ENV_HOME_ALT] {
        if let Ok(raw) = std::env::var(env) {
            let t = raw.trim();
            if !t.is_empty() {
                push(PathBuf::from(t));
            }
        }
    }

    // 2) Tauri store override (com.ccswitch.desktop / app_paths.json)
    if let Some(dir) = read_app_paths_override() {
        push(dir);
    }

    // 3) Default: {home}/.cc-switch
    let home = process_util::user_home();
    push(home.join(".cc-switch"));

    // 4) Windows legacy: HOME env when different from Profile and has db
    #[cfg(target_os = "windows")]
    {
        if let Ok(home_env) = std::env::var("HOME") {
            let trimmed = home_env.trim();
            if !trimmed.is_empty() {
                let legacy = PathBuf::from(trimmed).join(".cc-switch");
                push(legacy);
            }
        }
    }

    out
}

/// First existing `cc-switch.db` among candidates, plus all tried absolute paths.
pub fn resolve_cc_switch_db() -> (Option<PathBuf>, Vec<String>) {
    let candidates = candidate_cc_switch_dirs();
    let mut tried = Vec::new();
    for dir in candidates {
        let db = dir.join(DB_NAME);
        tried.push(db.display().to_string());
        if db.is_file() {
            // Windows legacy preference: prefer Profile default if both exist
            // (candidate order already puts Profile before HOME unless override).
            return (Some(db), tried);
        }
    }
    (None, tried)
}

fn normalize_path_key(path: &Path) -> String {
    let mut key = path.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    #[cfg(windows)]
    {
        key = key.to_ascii_lowercase();
    }
    key
}

/// Read `app_config_dir_override` from CC Switch Tauri store.
fn read_app_paths_override() -> Option<PathBuf> {
    let store_path = cc_switch_store_path()?;
    let text = fs::read_to_string(&store_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    // tauri-plugin-store may wrap values; try common shapes.
    let raw = extract_store_string(&v, STORE_KEY_OVERRIDE)?;
    let path = expand_user_path(&raw);
    if path.is_dir() || path.join(DB_NAME).is_file() {
        // Override may point at the config dir (contains db) or rarely the db parent.
        if path.join(DB_NAME).is_file() {
            return Some(path);
        }
        if path.is_file() && path.file_name().and_then(|s| s.to_str()) == Some(DB_NAME) {
            return path.parent().map(|p| p.to_path_buf());
        }
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

fn extract_store_string(root: &serde_json::Value, key: &str) -> Option<String> {
    // Flat: { "app_config_dir_override": "..." }
    if let Some(s) = root.get(key).and_then(|v| v.as_str()) {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    // Nested plugin shape: { "app_config_dir_override": { "value": "..." } }
    if let Some(s) = root
        .get(key)
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
    {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// Platform path for Tauri plugin-store file of CC Switch.
fn cc_switch_store_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = process_util::user_home();
        Some(
            home.join("Library/Application Support")
                .join(STORE_APP_ID)
                .join(STORE_FILE),
        )
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join(STORE_APP_ID).join(STORE_FILE));
        }
        Some(
            process_util::user_home()
                .join("AppData")
                .join("Roaming")
                .join(STORE_APP_ID)
                .join(STORE_FILE),
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            let t = xdg.trim();
            if !t.is_empty() {
                return Some(PathBuf::from(t).join(STORE_APP_ID).join(STORE_FILE));
            }
        }
        Some(
            process_util::user_home()
                .join(".local")
                .join("share")
                .join(STORE_APP_ID)
                .join(STORE_FILE),
        )
    }
}

fn expand_user_path(raw: &str) -> PathBuf {
    let raw = raw.trim();
    if raw == "~" {
        return process_util::user_home();
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        return process_util::user_home().join(rest);
    }
    PathBuf::from(raw)
}

// ── Scan / import ───────────────────────────────────────────────────────────

pub fn scan_cc_switch_providers() -> CcSwitchScanResult {
    let (db, tried) = resolve_cc_switch_db();
    let Some(db_path) = db else {
        return CcSwitchScanResult {
            status: "not_found".into(),
            db_path: None,
            tried_paths: tried,
            items: vec![],
            error: Some("CC Switch database not found".into()),
        };
    };

    match read_grokbuild_rows(&db_path) {
        Ok(rows) => {
            let atlas = load_atlas_index();
            let items: Vec<_> = rows_for_preview(rows)
                .into_iter()
                .map(|r| row_to_preview(r, &atlas))
                .collect();
            CcSwitchScanResult {
                status: "ok".into(),
                db_path: Some(db_path.display().to_string()),
                tried_paths: tried,
                items,
                error: None,
            }
        }
        Err(e) => CcSwitchScanResult {
            status: "error".into(),
            db_path: Some(db_path.display().to_string()),
            tried_paths: tried,
            items: vec![],
            error: Some(e),
        },
    }
}

pub fn import_cc_switch_providers(
    req: CcSwitchImportRequest,
) -> Result<CcSwitchImportResult, String> {
    import_cc_switch_providers_with(req)
}

fn import_cc_switch_providers_with(
    req: CcSwitchImportRequest,
) -> Result<CcSwitchImportResult, String> {
    // Default overwrite: re-importing the same slug updates key/base_url in place.
    let on_conflict = req
        .on_conflict
        .as_deref()
        .unwrap_or("overwrite")
        .trim()
        .to_ascii_lowercase();
    if req.source_ids.is_empty() {
        return Err("no providers selected".into());
    }

    let (db, _) = resolve_cc_switch_db();
    let db_path = db.ok_or_else(|| "CC Switch database not found".to_string())?;
    let rows = read_grokbuild_rows(&db_path)?;
    let by_id: HashMap<String, DbRow> = rows.into_iter().map(|r| (r.id.clone(), r)).collect();

    let mut imported = 0u32;
    let mut skipped = 0u32;
    let mut failed = Vec::new();
    let mut atlas = load_atlas_index();

    for sid in &req.source_ids {
        let Some(row) = by_id.get(sid) else {
            failed.push(CcSwitchImportFailure {
                source_id: sid.clone(),
                reason: "provider not found in CC Switch database".into(),
            });
            continue;
        };

        if is_official_category(row.category.as_deref()) {
            failed.push(CcSwitchImportFailure {
                source_id: sid.clone(),
                reason: "official Grok login cannot be imported".into(),
            });
            continue;
        }

        let parsed = match parse_settings_config(&row.settings_config, &row.name) {
            Ok(p) => p,
            Err(e) => {
                failed.push(CcSwitchImportFailure {
                    source_id: sid.clone(),
                    reason: e,
                });
                continue;
            }
        };

        if parsed.base_url.trim().is_empty() {
            failed.push(CcSwitchImportFailure {
                source_id: sid.clone(),
                reason: "empty base_url cannot be imported as custom".into(),
            });
            continue;
        }

        if parsed.api_key.trim().is_empty() {
            failed.push(CcSwitchImportFailure {
                source_id: sid.clone(),
                reason: "missing api_key".into(),
            });
            continue;
        }

        if is_proxy_managed(&parsed) {
            failed.push(CcSwitchImportFailure {
                source_id: sid.clone(),
                reason: "proxy-managed config (local proxy URL or placeholder key)".into(),
            });
            continue;
        }

        let mut id = slugify_id(&row.name, &parsed.profile);
        if atlas.enterprise.contains(&id) {
            failed.push(CcSwitchImportFailure {
                source_id: sid.clone(),
                reason: format!(
                    "id `{id}` is an enterprise-managed Atlas model and will not be overwritten"
                ),
            });
            continue;
        }
        if atlas.overwrite.contains(&id) {
            match on_conflict.as_str() {
                "overwrite" => {}
                "rename" => {
                    id = unique_id(&id, &atlas.taken());
                }
                _ => {
                    skipped += 1;
                    continue;
                }
            }
        }

        match merge_model_into_atlas(&id, sid, &parsed) {
            Ok(()) => {
                atlas.overwrite.insert(id);
                imported += 1;
            }
            Err(e) => {
                failed.push(CcSwitchImportFailure {
                    source_id: sid.clone(),
                    reason: e,
                });
            }
        }
    }

    // activate_id used to switch the App route. This import only merges model
    // tables into ~/.atlas and must not change [models].default.
    if req
        .activate_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|s| !s.is_empty())
    {
        failed.push(CcSwitchImportFailure {
            source_id: req.activate_id.unwrap_or_default(),
            reason: "import does not activate a route or change [models].default".into(),
        });
    }

    Ok(CcSwitchImportResult {
        imported,
        skipped,
        failed,
        providers: None,
    })
}

// ── DB + TOML parsing ───────────────────────────────────────────────────────

struct DbRow {
    id: String,
    name: String,
    website_url: Option<String>,
    category: Option<String>,
    settings_config: String,
    is_current: bool,
}

struct ParsedModel {
    profile: String,
    model: String,
    base_url: String,
    name: String,
    api_key: String,
    api_backend: String,
    /// Other CLI keys from the selected table, in source order.
    /// `app_*` and `env_key` are omitted.
    extra: Vec<(String, String)>,
}

struct AtlasIndex {
    overwrite: HashSet<String>,
    enterprise: HashSet<String>,
}

impl AtlasIndex {
    fn taken(&self) -> HashSet<String> {
        self.overwrite.union(&self.enterprise).cloned().collect()
    }

    fn empty() -> Self {
        Self {
            overwrite: HashSet::new(),
            enterprise: HashSet::new(),
        }
    }
}

fn is_official_category(category: Option<&str>) -> bool {
    category == Some("official")
}

fn rows_for_preview(rows: Vec<DbRow>) -> Vec<DbRow> {
    rows.into_iter()
        .filter(|r| !is_official_category(r.category.as_deref()))
        .collect()
}

fn read_grokbuild_rows(db_path: &Path) -> Result<Vec<DbRow>, String> {
    // Read-only open (no create). Prefer path form over URI for Windows drive letters.
    let conn =
        Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| {
            // Common when CC Switch holds a write lock.
            format!("open CC Switch db (read-only): {e}")
        })?;

    // Busy timeout if another process briefly locks.
    let _ = conn.busy_timeout(std::time::Duration::from_millis(1500));

    let mut stmt = conn
        .prepare(
            "SELECT id, name, website_url, category, settings_config, \
             COALESCE(is_current, 0) \
             FROM providers WHERE app_type = 'grokbuild' \
             ORDER BY sort_index IS NULL, sort_index, name",
        )
        .map_err(|e| format!("query providers: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(DbRow {
                id: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
                website_url: row.get::<_, Option<String>>(2)?,
                category: row.get::<_, Option<String>>(3)?,
                settings_config: row.get::<_, String>(4)?,
                is_current: row.get::<_, i64>(5).unwrap_or(0) != 0,
            })
        })
        .map_err(|e| format!("query map: {e}"))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("row: {e}"))?);
    }
    Ok(out)
}

fn row_to_preview(row: DbRow, atlas: &AtlasIndex) -> CcSwitchProviderPreview {
    let suggested = slugify_id(&row.name, "");
    let mut preview = CcSwitchProviderPreview {
        source_id: row.id.clone(),
        name: row.name.clone(),
        website_url: row.website_url.clone(),
        category: row.category.clone(),
        is_current: row.is_current,
        suggested_id: suggested.clone(),
        model: String::new(),
        base_url: String::new(),
        api_backend: "responses".into(),
        has_api_key: false,
        key_hint: None,
        status: "invalid".into(),
        status_detail: None,
    };

    if row.category.as_deref() == Some("official") {
        preview.status = "official".into();
        preview.status_detail = Some("Official Grok login — use Account tab".into());
        return preview;
    }

    match parse_settings_config(&row.settings_config, &row.name) {
        Ok(p) => {
            preview.model = p.model.clone();
            preview.base_url = p.base_url.clone();
            preview.api_backend = p.api_backend.clone();
            preview.has_api_key = !p.api_key.is_empty();
            preview.key_hint = key_hint(&p.api_key);
            if p.base_url.trim().is_empty() {
                preview.status = "invalid".into();
                preview.status_detail = Some("missing base_url".into());
            } else if p.api_key.trim().is_empty() {
                preview.status = "missing_key".into();
                preview.status_detail = Some("missing api_key".into());
            } else if is_proxy_managed(&p) {
                preview.status = "proxy_managed".into();
                preview.status_detail = Some("local proxy takeover — import may not work".into());
            } else if atlas.enterprise.contains(&suggested) {
                preview.status = "invalid".into();
                preview.status_detail = Some(format!(
                    "id `{suggested}` is an enterprise-managed Atlas model and will not be overwritten"
                ));
            } else if atlas.overwrite.contains(&suggested) {
                preview.status = "exists".into();
            } else {
                preview.status = "importable".into();
            }
        }
        Err(e) => {
            preview.status = "invalid".into();
            preview.status_detail = Some(e);
        }
    }
    preview
}

fn parse_settings_config(settings_json: &str, fallback_name: &str) -> Result<ParsedModel, String> {
    let v: serde_json::Value =
        serde_json::from_str(settings_json).map_err(|e| format!("settings_config json: {e}"))?;
    let config = v
        .get("config")
        .and_then(|c| c.as_str())
        .ok_or_else(|| "settings_config missing config string".to_string())?;
    parse_grok_toml(config, fallback_name)
}

/// Minimal TOML extract for Grok Build model tables (no full toml crate).
fn parse_grok_toml(config: &str, fallback_name: &str) -> Result<ParsedModel, String> {
    if config.trim().is_empty() {
        return Err("empty config.toml".into());
    }

    let mut default_profile: Option<String> = None;
    let mut in_models = false;
    let mut tables: HashMap<String, TableFields> = HashMap::new();
    let mut table_order: Vec<String> = Vec::new();
    let mut current_profile: Option<String> = None;

    for raw in config.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_models = false;
            current_profile = None;
            if line == "[models]" {
                in_models = true;
                continue;
            }
            // [model."grok-4.5"] or [model.foo]
            if let Some(rest) = line
                .strip_prefix("[model.")
                .and_then(|s| s.strip_suffix(']'))
            {
                let id = unquote_toml_key(rest);
                if !id.is_empty() {
                    if !tables.contains_key(&id) {
                        table_order.push(id.clone());
                    }
                    current_profile = Some(id);
                }
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = unquote_toml_value(v.trim());
        if in_models && key == "default" {
            default_profile = Some(val);
            continue;
        }
        if let Some(ref pid) = current_profile {
            tables.entry(pid.clone()).or_default().insert(key, val);
        }
    }

    let profile = default_profile
        .filter(|p| tables.contains_key(p))
        .or_else(|| table_order.into_iter().find(|id| tables.contains_key(id)))
        .ok_or_else(|| "no [model.<id>] table found".to_string())?;

    let fields = tables
        .get(&profile)
        .ok_or_else(|| format!("missing [model.\"{profile}\"]"))?;

    let model = fields
        .get("model")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| profile.clone());
    let base_url = fields.get("base_url").cloned().unwrap_or_default();
    let name = fields
        .get("name")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name.to_string());
    let mut api_key = fields.get("api_key").cloned().unwrap_or_default();
    if api_key.is_empty() {
        if let Some(env_key) = fields
            .get("env_key")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            if let Ok(v) = std::env::var(env_key) {
                api_key = v.trim().to_string();
            }
        }
    }
    let api_backend = fields
        .get("api_backend")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "responses".into());
    let extra = fields.extra_pairs();

    Ok(ParsedModel {
        profile,
        model,
        base_url,
        name,
        api_key,
        api_backend,
        extra,
    })
}

struct TableFields {
    order: Vec<String>,
    values: HashMap<String, String>,
}

impl TableFields {
    fn insert(&mut self, key: &str, value: String) {
        if !self.values.contains_key(key) {
            self.order.push(key.to_string());
        }
        self.values.insert(key.to_string(), value);
    }

    fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    fn extra_pairs(&self) -> Vec<(String, String)> {
        const SKIP: &[&str] = &[
            "model",
            "base_url",
            "name",
            "api_key",
            "api_backend",
            "env_key",
        ];
        self.order
            .iter()
            .filter(|k| !k.starts_with("app_") && !SKIP.contains(&k.as_str()))
            .filter_map(|k| {
                let v = self.values.get(k)?.trim();
                if v.is_empty() {
                    None
                } else {
                    Some((k.clone(), v.to_string()))
                }
            })
            .collect()
    }
}

impl Default for TableFields {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            values: HashMap::new(),
        }
    }
}

pub(crate) fn atlas_config_path() -> PathBuf {
    if let Ok(raw) = std::env::var(ATLAS_HOME_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("config.toml");
        }
    }
    crate::paths::shared_cli_home().join("config.toml")
}

fn load_atlas_index() -> AtlasIndex {
    let text = fs::read_to_string(atlas_config_path()).unwrap_or_default();
    atlas_index_from(&text)
}

fn atlas_index_from(text: &str) -> AtlasIndex {
    let mut overwrite = HashSet::new();
    let mut enterprise = HashSet::new();
    for section in providers::parse_model_sections_for_proxy(text) {
        if is_enterprise_managed(&section.fields) {
            enterprise.insert(section.id);
        } else {
            overwrite.insert(section.id);
        }
    }
    AtlasIndex {
        overwrite,
        enterprise,
    }
}

fn is_enterprise_managed(fields: &HashMap<String, String>) -> bool {
    if let Some(key) = fields.get("api_key") {
        let trimmed = key.trim();
        if trimmed.starts_with("ENC(") && trimmed.ends_with(')') {
            return true;
        }
    }
    if let Some(hidden) = fields.get("hidden") {
        let hidden = hidden.trim();
        if hidden.eq_ignore_ascii_case("true") || hidden == "1" {
            return true;
        }
    }
    false
}

fn merge_model_into_atlas(id: &str, source_id: &str, parsed: &ParsedModel) -> Result<(), String> {
    let path = atlas_config_path();
    let original = fs::read_to_string(&path).unwrap_or_default();
    let before_default = providers::get_models_default(&original);
    if providers::parse_model_sections_for_proxy(&original)
        .into_iter()
        .find(|section| section.id == id)
        .is_some_and(|section| is_enterprise_managed(&section.fields))
    {
        return Err(format!(
            "id `{id}` is an enterprise-managed Atlas model and will not be overwritten"
        ));
    }
    let stripped = remove_model_section(&original, id);
    let next = append_model_section(&stripped, id, &cli_fields(parsed, source_id));
    let after_default = providers::get_models_default(&next);
    if before_default != after_default {
        return Err("refusing to change [models].default".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, &next).map_err(|e| e.to_string())?;
    Ok(())
}

fn cli_fields(parsed: &ParsedModel, source_id: &str) -> Vec<(String, String)> {
    let mut out = vec![
        ("model".to_string(), parsed.model.clone()),
        ("base_url".to_string(), parsed.base_url.clone()),
        ("name".to_string(), parsed.name.clone()),
        ("api_key".to_string(), parsed.api_key.clone()),
        ("api_backend".to_string(), parsed.api_backend.clone()),
    ];
    for (key, value) in &parsed.extra {
        if out.iter().any(|(existing, _)| existing == key) {
            continue;
        }
        out.push((key.clone(), value.clone()));
    }
    out.push((CC_SOURCE_KEY.to_string(), source_id.to_string()));
    out
}

fn remove_model_section(text: &str, id: &str) -> String {
    let sections = providers::parse_model_sections_for_proxy(text);
    let Some(hit) = sections.iter().find(|s| s.id == id) else {
        return text.to_string();
    };
    let mut lines: Vec<&str> = text.lines().collect();
    let start = hit.start.min(lines.len());
    let end = hit.end.min(lines.len()).max(start);
    if start < end {
        lines.drain(start..end);
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn append_model_section(text: &str, id: &str, fields: &[(String, String)]) -> String {
    let body = fields
        .iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(key, value)| format!("{key} = {}", format_atlas_value(key, value)))
        .collect::<Vec<_>>()
        .join("\n");
    let header = format!(
        "[model.{}]",
        serde_json::to_string(id).unwrap_or_else(|_| format!("\"{id}\""))
    );
    let block = format!("{header}\n{body}\n");
    let base = text.trim_end();
    if base.is_empty() {
        block
    } else {
        format!("{base}\n\n{block}")
    }
}

fn format_atlas_value(key: &str, value: &str) -> String {
    if key == "context_window" {
        if let Ok(n) = value.trim().parse::<u64>() {
            return n.to_string();
        }
    }
    if key == "extra_headers" {
        let raw = value.trim();
        if raw.starts_with('{') {
            return raw.to_string();
        }
    }
    serde_json::to_string(value).unwrap_or_else(|_| format!("\"{value}\""))
}

fn unquote_toml_key(raw: &str) -> String {
    let t = raw.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn unquote_toml_value(raw: &str) -> String {
    let t = raw.trim();
    // strip trailing comments for simple cases: "x" # comment
    let t = if let Some(i) = t.find(" #") {
        t[..i].trim()
    } else {
        t
    };
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        // basic escape
        let inner = &t[1..t.len() - 1];
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        t.to_string()
    }
}

fn is_proxy_managed(p: &ParsedModel) -> bool {
    let key = p.api_key.trim();
    if key.eq_ignore_ascii_case("PROXY_MANAGED") || key == "proxy_managed" {
        return true;
    }
    let u = p.base_url.to_ascii_lowercase();
    u.contains("127.0.0.1:") || u.contains("localhost:")
}

fn key_hint(key: &str) -> Option<String> {
    let t = key.trim();
    if t.is_empty() {
        return None;
    }
    let tail: String = t
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    Some(format!("…{tail}"))
}

fn slugify_id(name: &str, profile: &str) -> String {
    let base = if !name.trim().is_empty() {
        name
    } else {
        profile
    };
    let id: String = base
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(48)
        .collect();
    if id.is_empty() || !id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        "imported".into()
    } else {
        id
    }
}

/// Stable error when `[model.<id>]` already exists in `~/.atlas/config.toml`.
pub const ATLAS_MODEL_ADD_DUPLICATE: &str = "duplicate_id";
/// Section id failed the charset check.
pub const ATLAS_MODEL_ADD_INVALID_ID: &str = "invalid_id";
/// `grok` and `official` stay reserved for the official route.
pub const ATLAS_MODEL_ADD_RESERVED_ID: &str = "reserved_id";
pub const ATLAS_MODEL_ADD_MISSING: &str = "missing_field";
pub const ATLAS_MODEL_ADD_BACKEND: &str = "invalid_backend";
pub const ATLAS_MODEL_ADD_CONTEXT: &str = "invalid_context_window";
pub const ATLAS_MODEL_ADD_BASE_URL: &str = "invalid_base_url";
pub const ATLAS_MODEL_ADD_DEFAULT: &str = "default_changed";
pub const ATLAS_MODEL_ADD_NOT_FOUND: &str = "not_found";
pub const ATLAS_MODEL_ADD_MANAGED: &str = "managed";
pub const ATLAS_MODEL_ADD_IS_DEFAULT: &str = "is_default";

/// One new enterprise-style catalog section. The key never leaves this process.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasModelAddRequest {
    pub id: String,
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    pub base_url: String,
    pub api_backend: String,
    pub context_window: u64,
    pub api_key: String,
}

/// Append `[model.<id>]` to the shared Atlas CLI config.
///
/// Does not change `[models].default`, does not write App agent-home, and
/// refuses an id that already exists (including enterprise `ENC` / hidden
/// sections). The upstream `model` value may repeat across sections.
pub fn add_atlas_catalog_model(req: AtlasModelAddRequest) -> Result<(), String> {
    crate::agent_home_config::with_config_write_lock(|| {
        add_atlas_catalog_model_at(&atlas_config_path(), req)
    })
}

pub(crate) fn add_atlas_catalog_model_at(
    path: &Path,
    req: AtlasModelAddRequest,
) -> Result<(), String> {
    let id = normalize_atlas_section_id(&req.id)?;
    let model = require_field(&req.model)?;
    let name = require_field(&req.name)?;
    let base_url = normalize_http_base(&req.base_url)?;
    let api_key = require_field(&req.api_key)?;
    let api_backend = normalize_atlas_backend(&req.api_backend)?;
    if req.context_window == 0 {
        return Err(ATLAS_MODEL_ADD_CONTEXT.into());
    }
    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");

    let original = fs::read_to_string(path).unwrap_or_default();
    let before_default = providers::get_models_default(&original);
    if providers::parse_model_sections_for_proxy(&original)
        .iter()
        .any(|section| section.id.eq_ignore_ascii_case(&id))
    {
        return Err(ATLAS_MODEL_ADD_DUPLICATE.into());
    }

    let mut fields = vec![
        ("model".to_string(), model),
        ("name".to_string(), name),
        ("description".to_string(), description.to_string()),
        ("base_url".to_string(), base_url),
        ("api_backend".to_string(), api_backend),
        ("context_window".to_string(), req.context_window.to_string()),
        ("api_key".to_string(), api_key),
    ];
    fields.retain(|(_, value)| !value.trim().is_empty());
    let next = append_model_section(&original, &id, &fields);
    let after_default = providers::get_models_default(&next);
    if before_default != after_default {
        return Err(ATLAS_MODEL_ADD_DEFAULT.into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, &next).map_err(|e| e.to_string())?;
    tracing::info!(target: "atlas_model", %id, "added atlas catalog model section");
    Ok(())
}

/// Editable view of one plaintext catalog section. The API key stays on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasModelEditView {
    pub id: String,
    pub model: String,
    pub name: String,
    pub description: String,
    pub base_url: String,
    pub api_backend: String,
    pub context_window: u64,
    pub has_api_key: bool,
}

/// Fields written back onto an existing section. An empty `api_key` keeps the stored key.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasModelUpdateRequest {
    pub id: String,
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    pub base_url: String,
    pub api_backend: String,
    pub context_window: u64,
    #[serde(default)]
    pub api_key: String,
}

/// Section ids in `text` that the settings list may edit or delete.
pub(crate) fn editable_section_ids(text: &str) -> HashSet<String> {
    providers::parse_model_sections_for_proxy(text)
        .into_iter()
        .filter(|section| !is_locked_atlas_section(&section.fields))
        .map(|section| section.id.to_ascii_lowercase())
        .collect()
}

fn is_locked_atlas_section(fields: &HashMap<String, String>) -> bool {
    if is_enterprise_managed(fields) {
        return true;
    }
    fields
        .get("managed")
        .map(|value| {
            let value = value.trim();
            value.eq_ignore_ascii_case("true") || value == "1"
        })
        .unwrap_or(false)
}

pub fn get_atlas_catalog_model(id: &str) -> Result<AtlasModelEditView, String> {
    let text = fs::read_to_string(atlas_config_path()).unwrap_or_default();
    get_atlas_catalog_model_in(&text, id)
}

pub(crate) fn get_atlas_catalog_model_in(
    text: &str,
    id: &str,
) -> Result<AtlasModelEditView, String> {
    let section = find_atlas_section(text, id)?;
    if is_locked_atlas_section(&section.fields) {
        return Err(ATLAS_MODEL_ADD_MANAGED.into());
    }
    let context_window = section
        .fields
        .get("context_window")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(200_000);
    let api_key = section
        .fields
        .get("api_key")
        .map(|value| value.trim())
        .unwrap_or("");
    Ok(AtlasModelEditView {
        id: section.id,
        model: section.fields.get("model").cloned().unwrap_or_default(),
        name: section.fields.get("name").cloned().unwrap_or_default(),
        description: section
            .fields
            .get("description")
            .cloned()
            .unwrap_or_default(),
        base_url: section.fields.get("base_url").cloned().unwrap_or_default(),
        api_backend: section
            .fields
            .get("api_backend")
            .cloned()
            .unwrap_or_else(|| "messages".into()),
        context_window,
        has_api_key: !api_key.is_empty(),
    })
}

pub fn update_atlas_catalog_model(req: AtlasModelUpdateRequest) -> Result<(), String> {
    crate::agent_home_config::with_config_write_lock(|| {
        update_atlas_catalog_model_at(&atlas_config_path(), req)
    })
}

pub(crate) fn update_atlas_catalog_model_at(
    path: &Path,
    req: AtlasModelUpdateRequest,
) -> Result<(), String> {
    let model = require_field(&req.model)?;
    let name = require_field(&req.name)?;
    let base_url = normalize_http_base(&req.base_url)?;
    let api_backend = normalize_atlas_backend(&req.api_backend)?;
    if req.context_window == 0 {
        return Err(ATLAS_MODEL_ADD_CONTEXT.into());
    }
    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");

    let original = fs::read_to_string(path).unwrap_or_default();
    let before_default = providers::get_models_default(&original);
    let section = find_atlas_section(&original, &req.id)?;
    if is_locked_atlas_section(&section.fields) {
        return Err(ATLAS_MODEL_ADD_MANAGED.into());
    }
    let api_key = {
        let incoming = req.api_key.trim();
        if incoming.is_empty() {
            section
                .fields
                .get("api_key")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ATLAS_MODEL_ADD_MISSING.to_string())?
        } else {
            incoming.to_string()
        }
    };

    let owned = [
        "model",
        "name",
        "description",
        "base_url",
        "api_backend",
        "context_window",
        "api_key",
    ];
    let mut fields = vec![
        ("model".to_string(), model),
        ("name".to_string(), name),
        ("description".to_string(), description.to_string()),
        ("base_url".to_string(), base_url),
        ("api_backend".to_string(), api_backend),
        ("context_window".to_string(), req.context_window.to_string()),
        ("api_key".to_string(), api_key),
    ];
    let mut extra: Vec<_> = section
        .fields
        .iter()
        .filter(|(key, value)| !owned.contains(&key.as_str()) && !value.trim().is_empty())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    extra.sort_by(|a, b| a.0.cmp(&b.0));
    fields.extend(extra);
    fields.retain(|(_, value)| !value.trim().is_empty());

    let removed = remove_model_section(&original, &section.id);
    let next = append_model_section(&removed, &section.id, &fields);
    if providers::get_models_default(&next) != before_default {
        return Err(ATLAS_MODEL_ADD_DEFAULT.into());
    }
    fs::write(path, &next).map_err(|e| e.to_string())?;
    tracing::info!(target: "atlas_model", id = %section.id, "updated atlas catalog model section");
    Ok(())
}

pub fn delete_atlas_catalog_model(id: &str) -> Result<(), String> {
    crate::agent_home_config::with_config_write_lock(|| {
        delete_atlas_catalog_model_at(&atlas_config_path(), id)
    })
}

pub(crate) fn delete_atlas_catalog_model_at(path: &Path, id: &str) -> Result<(), String> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let before_default = providers::get_models_default(&original);
    let section = find_atlas_section(&original, id)?;
    if is_locked_atlas_section(&section.fields) {
        return Err(ATLAS_MODEL_ADD_MANAGED.into());
    }
    if before_default
        .as_deref()
        .is_some_and(|default| default.eq_ignore_ascii_case(&section.id))
    {
        return Err(ATLAS_MODEL_ADD_IS_DEFAULT.into());
    }
    let next = remove_model_section(&original, &section.id);
    if providers::get_models_default(&next) != before_default {
        return Err(ATLAS_MODEL_ADD_DEFAULT.into());
    }
    fs::write(path, &next).map_err(|e| e.to_string())?;
    tracing::info!(target: "atlas_model", id = %section.id, "deleted atlas catalog model section");
    Ok(())
}

fn find_atlas_section(
    text: &str,
    id: &str,
) -> Result<crate::providers::ModelSection, String> {
    let needle = id.trim();
    if needle.is_empty() {
        return Err(ATLAS_MODEL_ADD_NOT_FOUND.into());
    }
    providers::parse_model_sections_for_proxy(text)
        .into_iter()
        .find(|section| section.id.eq_ignore_ascii_case(needle))
        .ok_or_else(|| ATLAS_MODEL_ADD_NOT_FOUND.to_string())
}

fn require_field(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        Err(ATLAS_MODEL_ADD_MISSING.into())
    } else {
        Ok(value.to_string())
    }
}

fn normalize_atlas_section_id(raw: &str) -> Result<String, String> {
    let id = raw.trim().to_ascii_lowercase();
    let ok = !id.is_empty()
        && id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
    if !ok {
        return Err(ATLAS_MODEL_ADD_INVALID_ID.into());
    }
    if providers::is_reserved_custom_provider_id(&id) {
        return Err(ATLAS_MODEL_ADD_RESERVED_ID.into());
    }
    Ok(id)
}

fn normalize_atlas_backend(raw: &str) -> Result<String, String> {
    match raw.trim() {
        "messages" => Ok("messages".into()),
        "chat_completions" => Ok("chat_completions".into()),
        "responses" => Ok("responses".into()),
        _ => Err(ATLAS_MODEL_ADD_BACKEND.into()),
    }
}

fn normalize_http_base(raw: &str) -> Result<String, String> {
    let value = raw.trim();
    let lower = value.to_ascii_lowercase();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(ATLAS_MODEL_ADD_BASE_URL.into());
    }
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(ATLAS_MODEL_ADD_BASE_URL.into());
    }
    Ok(value.to_string())
}

fn unique_id(base: &str, existing: &std::collections::HashSet<String>) -> String {
    if !existing.contains(base) {
        return base.to_string();
    }
    for n in 2..1000 {
        let cand = format!("{base}-{n}");
        if !existing.contains(&cand) {
            return cand;
        }
    }
    format!("{base}-x")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_amux_style_toml() {
        let toml = r#"[models]
default = "grok-4.5"

[model."grok-4.5"]
model = "grok-4.5"
base_url = "https://api.amux.ai/v1"
name = "Amux"
api_backend = "responses"
context_window = 500000
api_key = "sk-testkey1234"
"#;
        let p = parse_grok_toml(toml, "fallback").expect("parse");
        assert_eq!(p.profile, "grok-4.5");
        assert_eq!(p.base_url, "https://api.amux.ai/v1");
        assert_eq!(p.name, "Amux");
        assert_eq!(p.api_backend, "responses");
        assert_eq!(p.api_key, "sk-testkey1234");
        assert_eq!(p.model, "grok-4.5");
        assert_eq!(
            p.extra,
            vec![("context_window".to_string(), "500000".to_string())]
        );
    }

    #[test]
    fn drops_app_provider_mode_from_cli_fields() {
        let toml = r#"[models]
default = "grok-4.6"

[model."grok-4.6"]
model = "grok-4.6"
base_url = "https://relay.example/v1"
api_key = "runtime-key"
api_backend = "responses"
app_provider_mode = "grok_build_proxy"
"#;
        let parsed = parse_grok_toml(toml, "Relay").expect("parse");
        assert!(parsed.extra.iter().all(|(k, _)| k != "app_provider_mode"));
        let fields = cli_fields(&parsed, "source-relay");
        assert!(fields.iter().all(|(k, _)| k != "app_provider_mode"));
        assert!(fields
            .iter()
            .any(|(k, v)| k == CC_SOURCE_KEY && v == "source-relay"));
    }

    #[test]
    fn slugify_and_unique() {
        assert_eq!(slugify_id("Amux", ""), "amux");
        assert_eq!(slugify_id("My Relay!", ""), "my-relay");
        let mut set = std::collections::HashSet::new();
        set.insert("amux".into());
        assert_eq!(unique_id("amux", &set), "amux-2");
    }

    #[test]
    fn proxy_managed_detected() {
        let p = ParsedModel {
            profile: "x".into(),
            model: "x".into(),
            base_url: "http://127.0.0.1:15721/grokbuild/v1".into(),
            name: "x".into(),
            api_key: "PROXY_MANAGED".into(),
            api_backend: "responses".into(),
            extra: Vec::new(),
        };
        assert!(is_proxy_managed(&p));
    }

    #[test]
    fn key_hint_tail() {
        assert_eq!(key_hint("sk-abcdefgh").as_deref(), Some("…efgh"));
    }

    #[test]
    fn candidate_dirs_include_default_dot_cc_switch() {
        let dirs = candidate_cc_switch_dirs();
        assert!(
            dirs.iter().any(|p| p.ends_with(".cc-switch")),
            "expected ~/.cc-switch among {dirs:?}"
        );
    }

    #[test]
    fn parse_settings_json_wrapper() {
        let json = serde_json::json!({
            "config": "[models]\ndefault = \"m\"\n\n[model.\"m\"]\nmodel = \"m\"\nbase_url = \"https://ex.com/v1\"\nname = \"N\"\napi_key = \"k\"\napi_backend = \"responses\"\n"
        })
        .to_string();
        let p = parse_settings_config(&json, "N").expect("ok");
        assert_eq!(p.base_url, "https://ex.com/v1");
    }

    #[test]
    fn scan_temp_db_with_amux_row() {
        let dir = tempfile_dir();
        let db = dir.join(DB_NAME);
        {
            let conn = Connection::open(&db).expect("create db");
            conn.execute_batch(
                "CREATE TABLE providers (
                    id TEXT NOT NULL,
                    app_type TEXT NOT NULL,
                    name TEXT NOT NULL,
                    settings_config TEXT NOT NULL,
                    website_url TEXT,
                    category TEXT,
                    sort_index INTEGER,
                    is_current BOOLEAN NOT NULL DEFAULT 0,
                    PRIMARY KEY (id, app_type)
                );",
            )
            .expect("schema");
            let cfg = serde_json::json!({
                "config": "[models]\ndefault = \"grok-4.5\"\n\n[model.\"grok-4.5\"]\nmodel = \"grok-4.5\"\nbase_url = \"https://api.amux.ai/v1\"\nname = \"Amux\"\napi_backend = \"responses\"\napi_key = \"sk-secretXXXX\"\n"
            })
            .to_string();
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, website_url, category, is_current)
                 VALUES (?1, 'grokbuild', 'Amux', ?2, 'https://amux.ai', 'aggregator', 1)",
                rusqlite::params!["uuid-amux", cfg],
            )
            .expect("insert");
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, website_url, category, is_current)
                 VALUES ('grokbuild-official', 'grokbuild', 'Grok Official', '{\"config\":\"\"}', 'https://x.ai/grok', 'official', 0)",
                [],
            )
            .expect("insert official");
        }

        let rows = read_grokbuild_rows(&db).expect("read");
        assert_eq!(rows.len(), 2);
        let previews: Vec<_> = rows_for_preview(rows)
            .into_iter()
            .map(|r| row_to_preview(r, &AtlasIndex::empty()))
            .collect();
        assert_eq!(previews.len(), 1);
        let amux = &previews[0];
        assert_eq!(amux.name, "Amux");
        assert_eq!(amux.status, "importable");
        assert_eq!(amux.base_url, "https://api.amux.ai/v1");
        assert!(amux.has_api_key);
        assert!(previews
            .iter()
            .all(|p| p.category.as_deref() != Some("official")));
    }

    #[test]
    fn import_merges_into_atlas_without_touching_default_or_enterprise() {
        let _lock = crate::paths::APP_HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let atlas_home = tempfile_dir();
        let cc_home = tempfile_dir();
        let _restore = RestoreEnv::set(&[
            (ATLAS_HOME_ENV, atlas_home.to_string_lossy().as_ref()),
            (ENV_DIR, cc_home.to_string_lossy().as_ref()),
        ]);

        let prior = "\
[models]
default = \"arch-kimi-for-coding\"

[model.\"arch-kimi-for-coding\"]
name = \"Kimi\"
api_key = \"ENC(secret)\"

[model.\"hidden-one\"]
name = \"Hidden\"
api_key = \"sk-hidden\"
hidden = true

[model.\"relay\"]
model = \"old\"
base_url = \"https://old.example/v1\"
name = \"Relay\"
api_key = \"old-key\"
api_backend = \"responses\"
";
        fs::write(atlas_home.join("config.toml"), prior).expect("seed atlas");

        let db = cc_home.join(DB_NAME);
        let conn = Connection::open(&db).expect("create db");
        conn.execute_batch(
            "CREATE TABLE providers (
                id TEXT NOT NULL,
                app_type TEXT NOT NULL,
                name TEXT NOT NULL,
                settings_config TEXT NOT NULL,
                website_url TEXT,
                category TEXT,
                sort_index INTEGER,
                is_current BOOLEAN NOT NULL DEFAULT 0,
                PRIMARY KEY (id, app_type)
            );",
        )
        .expect("schema");
        let relay_cfg = "[models]\ndefault = \"grok-4.6\"\n\n[model.\"grok-4.6\"]\nmodel = \"grok-4.6\"\nbase_url = \"https://relay.example/v1\"\nname = \"Relay\"\napi_backend = \"responses\"\napi_key = \"runtime-key\"\ncontext_window = 500000\nextra_headers = { \"User-Agent\" = \"atlas\" }\napp_provider_mode = \"grok_build_proxy\"\n";
        let amux_cfg = "[models]\ndefault = \"grok-4.5\"\n\n[model.\"grok-4.5\"]\nmodel = \"grok-4.5\"\nbase_url = \"https://api.amux.ai/v1\"\nname = \"Amux\"\napi_backend = \"responses\"\napi_key = \"sk-amux\"\n";
        let hidden_cfg = "[models]\ndefault = \"x\"\n\n[model.\"x\"]\nmodel = \"x\"\nbase_url = \"https://hidden.example/v1\"\nname = \"Hidden One\"\napi_key = \"sk-new\"\napi_backend = \"responses\"\n";
        for (id, name, cfg, category) in [
            ("source-relay", "Relay", relay_cfg, "aggregator"),
            ("source-amux", "Amux", amux_cfg, "aggregator"),
            ("source-hidden", "hidden-one", hidden_cfg, "custom"),
            ("grokbuild-official", "Grok Official", "", "official"),
        ] {
            let settings = serde_json::json!({ "config": cfg }).to_string();
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, category, is_current)
                 VALUES (?1, 'grokbuild', ?2, ?3, ?4, 0)",
                rusqlite::params![id, name, settings, category],
            )
            .expect("insert");
        }
        drop(conn);

        let result = import_cc_switch_providers_with(CcSwitchImportRequest {
            source_ids: vec![
                "source-relay".into(),
                "source-amux".into(),
                "source-hidden".into(),
                "grokbuild-official".into(),
            ],
            on_conflict: Some("overwrite".into()),
            activate_id: None,
        })
        .expect("import");
        assert_eq!(result.imported, 2, "{result:?}");
        assert_eq!(result.failed.len(), 2, "{result:?}");
        assert!(result.providers.is_none());
        assert!(result
            .failed
            .iter()
            .any(|f| f.source_id == "grokbuild-official"));
        assert!(result
            .failed
            .iter()
            .any(|f| f.source_id == "source-hidden" && f.reason.contains("enterprise-managed")));

        let written = fs::read_to_string(atlas_home.join("config.toml")).expect("atlas config");
        assert!(written.contains("default = \"arch-kimi-for-coding\""));
        assert!(written.contains("api_key = \"ENC(secret)\""));
        assert!(written.contains("hidden = true"));
        assert!(written.contains("base_url = \"https://relay.example/v1\""));
        assert!(written.contains("base_url = \"https://api.amux.ai/v1\""));
        assert!(written.contains("context_window = 500000"));
        assert!(written.contains("extra_headers = { \"User-Agent\" = \"atlas\" }"));
        assert!(written.contains("app_cc_switch_source = \"source-relay\""));
        assert!(!written.contains("127.0.0.1"));
        assert!(!written.contains("app_provider_mode"));
        assert!(!written.contains("https://old.example/v1"));
        assert!(!written.contains("https://hidden.example/v1"));
        assert!(providers::get_models_default(&written).as_deref() == Some("arch-kimi-for-coding"));

        let _ = fs::remove_dir_all(&atlas_home);
        let _ = fs::remove_dir_all(&cc_home);
    }

    fn tempfile_dir() -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!(
            "grok-app-cc-switch-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&d).expect("mkdir");
        d
    }

    #[test]
    fn expand_tilde() {
        let p = expand_user_path("~/foo/bar");
        assert!(p.ends_with("foo/bar") || p.ends_with(r"foo\bar"));
    }

    struct RestoreEnv(Vec<(&'static str, Option<String>)>);

    impl RestoreEnv {
        fn set(values: &[(&'static str, &str)]) -> Self {
            let previous = values
                .iter()
                .map(|(key, value)| {
                    let old = std::env::var(key).ok();
                    std::env::set_var(key, value);
                    (*key, old)
                })
                .collect();
            Self(previous)
        }
    }

    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            for (key, value) in self.0.drain(..).rev() {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    #[allow(dead_code)]
    fn write_file(path: &Path, body: &str) {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    fn sample_add(id: &str) -> AtlasModelAddRequest {
        AtlasModelAddRequest {
            id: id.into(),
            model: "kimi-for-coding".into(),
            name: id.into(),
            description: Some(String::new()),
            base_url: "https://api.kimi.com/coding/v1".into(),
            api_backend: "messages".into(),
            context_window: 200_000,
            api_key: "sk-test-key".into(),
        }
    }

    #[test]
    fn add_appends_section_and_keeps_default() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "[models]\ndefault = \"arch-glm-5.2\"\n\n[model.keep]\nmodel = \"keep\"\nname = \"keep\"\nbase_url = \"https://keep.example/v1\"\napi_key = \"ENC(secret)\"\n",
        )
        .unwrap();
        add_atlas_catalog_model_at(&path, sample_add("Arch-Kimi-New")).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("default = \"arch-glm-5.2\""));
        assert!(text.contains("[model.\"arch-kimi-new\"]"));
        assert!(text.contains("context_window = 200000\n"));
        assert!(!text.contains("context_window = \"200000\""));
        assert!(text.contains("api_backend = \"messages\""));
        assert!(!text.contains("description ="));
        assert!(text.contains("api_key = \"ENC(secret)\""));
        assert_eq!(
            providers::get_models_default(&text).as_deref(),
            Some("arch-glm-5.2")
        );
    }

    #[test]
    fn add_rejects_duplicate_id_without_rewriting() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        let original = "[models]\ndefault = \"grok\"\n\n[model.arch-kimi-code-eric1]\nmodel = \"kimi-for-coding\"\nname = \"eric\"\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"sk-old\"\n";
        fs::write(&path, original).unwrap();
        let err = add_atlas_catalog_model_at(&path, sample_add("ARCH-KIMI-CODE-ERIC1"))
            .expect_err("duplicate");
        assert_eq!(err, ATLAS_MODEL_ADD_DUPLICATE);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn add_rejects_enterprise_section_of_the_same_id() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        let original = "[model.arch-kimi-code-eric]\nmodel = \"ENC(abc)\"\nname = \"eric\"\napi_key = \"ENC(secret)\"\nhidden = true\n";
        fs::write(&path, original).unwrap();
        let err = add_atlas_catalog_model_at(&path, sample_add("arch-kimi-code-eric"))
            .expect_err("enterprise duplicate");
        assert_eq!(err, ATLAS_MODEL_ADD_DUPLICATE);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn add_rejects_reserved_and_blank_ids() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        fs::write(&path, "").unwrap();
        assert_eq!(
            add_atlas_catalog_model_at(&path, sample_add("grok")).unwrap_err(),
            ATLAS_MODEL_ADD_RESERVED_ID
        );
        assert_eq!(
            add_atlas_catalog_model_at(&path, sample_add("official")).unwrap_err(),
            ATLAS_MODEL_ADD_RESERVED_ID
        );
        assert_eq!(
            add_atlas_catalog_model_at(&path, sample_add("bad id")).unwrap_err(),
            ATLAS_MODEL_ADD_INVALID_ID
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    fn sample_update(id: &str, api_key: &str) -> AtlasModelUpdateRequest {
        AtlasModelUpdateRequest {
            id: id.into(),
            model: "kimi-for-coding".into(),
            name: "Kimi".into(),
            description: Some("note".into()),
            base_url: "https://api.kimi.com/coding/v1".into(),
            api_backend: "messages".into(),
            context_window: 200_000,
            api_key: api_key.into(),
        }
    }

    #[test]
    fn update_rewrites_plaintext_section_and_keeps_key_and_default() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "[models]\ndefault = \"arch-glm-5.2\"\n\n[model.keep]\napi_key = \"ENC(secret)\"\nmanaged = true\n\n[model.arch-kimi-new]\nmodel = \"old\"\nname = \"old\"\nbase_url = \"https://old.example/v1\"\napi_backend = \"responses\"\ncontext_window = 1000\napi_key = \"sk-old\"\nextra = \"kept\"\n",
        )
        .unwrap();
        let view = get_atlas_catalog_model_in(&fs::read_to_string(&path).unwrap(), "ARCH-KIMI-NEW")
            .unwrap();
        assert_eq!(view.model, "old");
        assert!(view.has_api_key);
        let encoded = serde_json::to_string(&view).unwrap();
        assert!(!encoded.contains("sk-old"));
        update_atlas_catalog_model_at(&path, sample_update("arch-kimi-new", "")).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("default = \"arch-glm-5.2\""));
        assert!(text.contains("name = \"Kimi\""));
        assert!(text.contains("context_window = 200000\n"));
        assert!(!text.contains("context_window = \"200000\""));
        assert!(text.contains("api_key = \"sk-old\""));
        assert!(text.contains("extra = \"kept\""));
        assert!(text.contains("api_key = \"ENC(secret)\""));
        assert_eq!(
            providers::get_models_default(&text).as_deref(),
            Some("arch-glm-5.2")
        );
    }

    #[test]
    fn update_rejects_managed_sections() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        let original = "[model.arch-kimi-code-eric]\nmodel = \"kimi\"\nname = \"eric\"\napi_key = \"ENC(secret)\"\nmanaged = true\n";
        fs::write(&path, original).unwrap();
        assert_eq!(
            update_atlas_catalog_model_at(&path, sample_update("arch-kimi-code-eric", "sk-new"))
                .unwrap_err(),
            ATLAS_MODEL_ADD_MANAGED
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(!editable_section_ids(original).contains("arch-kimi-code-eric"));
    }

    #[test]
    fn delete_removes_plaintext_section_and_keeps_default() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "[models]\ndefault = \"arch-glm-5.2\"\n\n[model.arch-glm-5.2]\nmodel = \"glm\"\nname = \"glm\"\napi_key = \"ENC(secret)\"\nmanaged = true\n\n[model.arch-kimi-new]\nmodel = \"kimi-for-coding\"\nname = \"Kimi\"\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"sk-old\"\n",
        )
        .unwrap();
        assert!(editable_section_ids(&fs::read_to_string(&path).unwrap()).contains("arch-kimi-new"));
        delete_atlas_catalog_model_at(&path, "ARCH-KIMI-NEW").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("arch-kimi-new"));
        assert!(text.contains("api_key = \"ENC(secret)\""));
        assert_eq!(
            providers::get_models_default(&text).as_deref(),
            Some("arch-glm-5.2")
        );
    }

    #[test]
    fn delete_rejects_default_and_managed_sections() {
        let dir = tempfile_dir();
        let path = dir.join("config.toml");
        let original = "[models]\ndefault = \"arch-kimi-new\"\n\n[model.arch-kimi-new]\nmodel = \"kimi\"\nname = \"Kimi\"\napi_key = \"sk-old\"\n\n[model.locked]\nmodel = \"locked\"\nname = \"locked\"\napi_key = \"plain\"\nmanaged = true\n";
        fs::write(&path, original).unwrap();
        assert_eq!(
            delete_atlas_catalog_model_at(&path, "arch-kimi-new").unwrap_err(),
            ATLAS_MODEL_ADD_IS_DEFAULT
        );
        assert_eq!(
            delete_atlas_catalog_model_at(&path, "locked").unwrap_err(),
            ATLAS_MODEL_ADD_MANAGED
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}
