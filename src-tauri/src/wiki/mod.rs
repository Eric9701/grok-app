//! Wiki engine: project pages, keyword search, and a one-shot model call.
//!
//! The desktop session, Relay, and `~/.atlas` writes stay outside this module.

mod complete;
mod gate;
mod lint;
mod path;
mod search;
mod usage;

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

pub use complete::{headless_args, resolve_model_id, WikiUsage};
pub use gate::{WikiOpen, WikiPageInput, WikiText};
pub use lint::WikiLint;
pub use search::WikiHit;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiComplete {
    pub text: String,
    pub model_id: String,
    pub usage: Option<WikiUsage>,
}

#[tauri::command]
pub fn wiki_open(project: String) -> Result<WikiOpen, String> {
    gate::open(PathBuf::from(project).as_path())
}

#[tauri::command]
pub fn wiki_read(project: String, path: String) -> Result<WikiText, String> {
    gate::read(PathBuf::from(project).as_path(), &path)
}

#[tauri::command]
pub fn wiki_write_pages(
    project: String,
    pages: Vec<WikiPageInput>,
    allow_overwrite: Option<bool>,
) -> Result<Vec<String>, String> {
    gate::write_pages(
        PathBuf::from(project).as_path(),
        &pages,
        allow_overwrite.unwrap_or(false),
    )
}

#[tauri::command]
pub fn wiki_remove_page(project: String, path: String) -> Result<(), String> {
    gate::remove_page(PathBuf::from(project).as_path(), &path)
}

#[tauri::command]
pub fn wiki_list(project: String) -> Result<Vec<String>, String> {
    lint::list_pages(PathBuf::from(project).as_path())
}

#[tauri::command]
pub fn wiki_list_raw(project: String) -> Result<Vec<String>, String> {
    lint::list_raw(PathBuf::from(project).as_path())
}

#[tauri::command]
pub fn wiki_lint(project: String) -> Result<WikiLint, String> {
    lint::lint(PathBuf::from(project).as_path())
}

#[tauri::command]
pub fn wiki_search(
    project: String,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<WikiHit>, String> {
    search::search(
        PathBuf::from(project).as_path(),
        &query,
        limit.unwrap_or(8) as usize,
    )
}

#[tauri::command]
pub fn wiki_fingerprint(project: String, path: String) -> Result<String, String> {
    gate::fingerprint(PathBuf::from(project).as_path(), &path)
}

#[tauri::command]
pub fn wiki_cache_get(project: String, key: String) -> Result<Option<String>, String> {
    gate::cache_get(PathBuf::from(project).as_path(), &key)
}

#[tauri::command]
pub fn wiki_cache_put(project: String, key: String, value: String) -> Result<(), String> {
    gate::cache_put(PathBuf::from(project).as_path(), &key, &value)
}

/// Headless completion. Does not append to a desktop session and does not
/// rewrite the agent `config.toml`.
#[tauri::command]
pub async fn wiki_complete(
    prompt: String,
    project: String,
    model_id: Option<String>,
) -> Result<WikiComplete, String> {
    tauri::async_runtime::spawn_blocking(move || complete_prompt(prompt, project, model_id))
        .await
        .map_err(|_| "wiki_complete_join".to_string())?
}

fn complete_prompt(
    prompt: String,
    project: String,
    model_id: Option<String>,
) -> Result<WikiComplete, String> {
    let settings = crate::store::load_settings();
    let workdir = path::require_project(PathBuf::from(&project).as_path())?;
    let (fallback, _) = complete::configured_model()?;
    let model = model_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .unwrap_or(fallback);
    let home = complete::home_for_wiki_model(&model, &settings.session_data_mode);
    let probe = crate::cli_probe::probe_cli(settings.manual_cli_path.as_deref());
    let cli = probe
        .path
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| "wiki_cli_missing".to_string())?;
    let parsed = complete::run(
        std::path::Path::new(&cli),
        &home,
        &model,
        &prompt,
        &workdir,
        Duration::from_secs(180),
    )?;
    Ok(WikiComplete {
        text: parsed.text,
        model_id: model,
        usage: parsed.usage,
    })
}

#[tauri::command]
pub fn wiki_usage_list(project: String) -> Result<Vec<usage::WikiUsageEntry>, String> {
    usage::list(PathBuf::from(project).as_path())
}

#[tauri::command]
pub fn wiki_usage_append(
    project: String,
    source: String,
    model_id: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    reported: Option<bool>,
) -> Result<Vec<usage::WikiUsageEntry>, String> {
    usage::append(
        PathBuf::from(project).as_path(),
        &source,
        model_id.as_deref().unwrap_or(""),
        input_tokens.unwrap_or(0),
        output_tokens.unwrap_or(0),
        total_tokens.unwrap_or(0),
        reported.unwrap_or(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atlas-wiki-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_escape_and_keeps_the_batch_unwritten() {
        let dir = scratch();
        let err = gate::write_pages(
            &dir,
            &[
                WikiPageInput {
                    path: "wiki/ok.md".into(),
                    content: "kept".into(),
                },
                WikiPageInput {
                    path: "wiki/../secret.md".into(),
                    content: "no".into(),
                },
            ],
            false,
        )
        .unwrap_err();
        assert_eq!(err, "wiki_path");
        assert!(!dir.join("wiki/ok.md").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrite_is_refused_until_allowed() {
        let dir = scratch();
        gate::write_pages(
            &dir,
            &[WikiPageInput {
                path: "wiki/note.md".into(),
                content: "one".into(),
            }],
            false,
        )
        .unwrap();
        let err = gate::write_pages(
            &dir,
            &[WikiPageInput {
                path: "wiki/note.md".into(),
                content: "two".into(),
            }],
            false,
        )
        .unwrap_err();
        assert_eq!(err, "wiki_overwrite");
        assert_eq!(fs::read_to_string(dir.join("wiki/note.md")).unwrap(), "one");
        gate::write_pages(
            &dir,
            &[WikiPageInput {
                path: "wiki/note.md".into(),
                content: "two".into(),
            }],
            true,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(dir.join("wiki/note.md")).unwrap(), "two");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lint_reports_a_broken_link_and_an_orphan() {
        let dir = scratch();
        gate::write_pages(
            &dir,
            &[
                WikiPageInput {
                    path: "wiki/index.md".into(),
                    content: "See [[alpha]] and [[missing]].".into(),
                },
                WikiPageInput {
                    path: "wiki/concepts/alpha.md".into(),
                    content: "# Alpha\n".into(),
                },
                WikiPageInput {
                    path: "wiki/concepts/alone.md".into(),
                    content: "# Alone\n".into(),
                },
            ],
            false,
        )
        .unwrap();
        let listed = lint::list_pages(&dir).unwrap();
        assert!(listed.iter().any(|p| p == "wiki/concepts/alone.md"));
        let report = lint::lint(&dir).unwrap();
        assert!(report.broken.iter().any(|b| b.target == "missing"));
        assert!(report.orphans.iter().any(|p| p.ends_with("alone.md")));
        assert!(!report.orphans.iter().any(|p| p.ends_with("alpha.md")));
        assert!(!report.orphans.iter().any(|p| p.ends_with("index.md")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_ranks_a_title_match() {
        let dir = scratch();
        gate::write_pages(
            &dir,
            &[WikiPageInput {
                path: "wiki/concepts/pump.md".into(),
                content: "# 水泵\n\nA pump moves water.\n".into(),
            }],
            false,
        )
        .unwrap();
        let hits = search::search(&dir, "水泵", 5).unwrap();
        assert_eq!(hits[0].path, "wiki/concepts/pump.md");
        assert!(hits[0].score >= 10);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn model_resolution_reads_settings_then_config_default() {
        assert_eq!(
            resolve_model_id(Some("grok-4.6"), "[models]\ndefault = \"other\"\n").unwrap(),
            "grok-4.6"
        );
        assert_eq!(
            resolve_model_id(None, "[models]\ndefault = \"atlas-main\"\n").unwrap(),
            "atlas-main"
        );
        assert_eq!(
            resolve_model_id(None, "").unwrap_err(),
            "wiki_model_missing"
        );
        let args = headless_args("atlas-main", "prompt.txt");
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "--prompt-file" && pair[1] == "prompt.txt"));
        assert!(!args.iter().any(|arg| arg == "-p"));
        assert!(args.iter().any(|a| a == "atlas-main"));
        assert!(args.iter().any(|a| a == "json"));
        assert!(args.windows(2).any(|pair| pair == ["--max-turns", "2"]));
        assert!(args.iter().any(|a| a == "--no-subagents"));
        assert!(!args.iter().any(|a| a == "--always-approve"));
        assert!(!args.iter().any(|a| a.contains("config.toml")));
    }

    #[test]
    fn raw_listing_skips_hidden_names() {
        let dir = scratch();
        fs::create_dir_all(dir.join("raw/sources")).unwrap();
        fs::write(dir.join("raw/sources/note.md"), "hello").unwrap();
        fs::write(dir.join("raw/.secret.md"), "no").unwrap();
        fs::create_dir_all(dir.join("wiki")).unwrap();
        fs::write(dir.join("wiki/index.md"), "index").unwrap();
        let listed = lint::list_raw(&dir).unwrap();
        assert_eq!(listed, vec!["raw/sources/note.md".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_page_deletes_only_that_wiki_file() {
        let dir = scratch();
        gate::write_pages(
            &dir,
            &[WikiPageInput {
                path: "wiki/sources/note.md".into(),
                content: "Couldn't set model\n".into(),
            }],
            false,
        )
        .unwrap();
        gate::write_pages(
            &dir,
            &[WikiPageInput {
                path: "wiki/index.md".into(),
                content: "# Index\n".into(),
            }],
            false,
        )
        .unwrap();
        gate::remove_page(&dir, "wiki/sources/note.md").unwrap();
        let listed = lint::list_pages(&dir).unwrap();
        assert!(listed.iter().any(|p| p == "wiki/index.md"));
        assert!(!listed.iter().any(|p| p.ends_with("note.md")));
        assert!(gate::remove_page(&dir, "wiki/../secret.md").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compile_workdir_is_the_knowledge_base() {
        let dir = scratch();
        let resolved = path::require_project(&dir).unwrap();
        assert_eq!(resolved, dir.canonicalize().unwrap());
        assert!(path::require_project(std::path::Path::new("notes")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
