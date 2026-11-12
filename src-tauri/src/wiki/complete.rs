//! One-shot Atlas completion for wiki ingest.
//!
//! Spawns the CLI directly. It does not touch the open desktop session and
//! does not write `config.toml`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

/// Emitted after the Atlas CLI process is running, before it returns.
pub const WIKI_CLI_STARTED_EVENT: &str = "wiki://cli-started";
pub const WIKI_CLI_PULSE_EVENT: &str = "wiki://cli-pulse";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiCliPulse {
    pub project: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WikiUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedComplete {
    pub text: String,
    pub usage: Option<WikiUsage>,
}

pub fn resolve_model_id(settings_model: Option<&str>, config_text: &str) -> Result<String, String> {
    if let Some(id) = settings_model.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(id.to_string());
    }
    crate::providers::get_models_default(config_text)
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "wiki_model_missing".to_string())
}

/// Compile turns outside 1–20 become the default of 6.
pub fn clamp_wiki_compile_turns(raw: u32) -> u32 {
    if (1..=20).contains(&raw) {
        raw
    } else {
        6
    }
}

/// Argument list for a text-only compile. No config path and no `--always-approve`.
///
/// `--max-turns 1` treats reasoning as the whole turn and returns no reply.
/// Tools stay disallowed so the child cannot wait on an approval prompt.
pub fn headless_args(model_id: &str, prompt_file: &str, max_turns: u32) -> Vec<String> {
    vec![
        "--no-auto-update".into(),
        "--prompt-file".into(),
        prompt_file.to_string(),
        "-m".into(),
        model_id.to_string(),
        "--max-turns".into(),
        clamp_wiki_compile_turns(max_turns).to_string(),
        "--no-plan".into(),
        "--no-subagents".into(),
        "--disable-web-search".into(),
        "--disallowed-tools".into(),
        "run_terminal_cmd,run_terminal_command,web_search,web_fetch,search_replace,write,edit,read_file,Read,grep,glob,list_dir,search_tool,NotebookRead,Agent,spawn_subagent,bash,bash_tool".into(),
        "--output-format".into(),
        "json".into(),
    ]
}

/// Pull the reply and token usage out of a headless stdout.
/// Plain text stays the reply and carries no invented usage.
pub fn parse_complete_output(stdout: &str) -> ParsedComplete {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return ParsedComplete {
            text: String::new(),
            usage: None,
        };
    }
    if let Some(parsed) = parse_json_body(trimmed) {
        if parsed.text.trim().is_empty() && trimmed.contains("---FILE:") {
            return ParsedComplete {
                text: trimmed.to_string(),
                usage: parsed.usage,
            };
        }
        return parsed;
    }
    ParsedComplete {
        text: trimmed.to_string(),
        usage: None,
    }
}

fn parse_json_body(raw: &str) -> Option<ParsedComplete> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return Some(ParsedComplete {
            text: text_from(&value).unwrap_or_default(),
            usage: usage_from(&value),
        });
    }
    let mut text = String::new();
    let mut usage = None;
    let mut saw_json = false;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        saw_json = true;
        if let Some(next) = usage_from(&value) {
            usage = Some(next);
        }
        if let Some(piece) = text_from(&value) {
            if !piece.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&piece);
            }
        }
    }
    if saw_json {
        Some(ParsedComplete { text, usage })
    } else {
        None
    }
}

fn text_from(value: &Value) -> Option<String> {
    for key in ["result", "text", "message", "output", "content", "response"] {
        if let Some(text) = value.get(key).and_then(|item| item.as_str()) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    let kind = value
        .get("type")
        .or_else(|| value.get("event"))
        .and_then(|item| item.as_str())
        .unwrap_or("");
    if kind == "text" || kind == "assistant" || kind == "content_block_delta" {
        for key in ["data", "text"] {
            if let Some(text) = value.get(key).and_then(|item| item.as_str()) {
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
    }
    None
}

fn usage_from(value: &Value) -> Option<WikiUsage> {
    let obj = value
        .get("usage")
        .or_else(|| value.get("tokenUsage"))
        .or_else(|| value.get("token_usage"))
        .unwrap_or(value);
    let input = first_u64(
        obj,
        &[
            "inputTokens",
            "input_tokens",
            "promptTokens",
            "prompt_tokens",
        ],
    );
    let output = first_u64(
        obj,
        &[
            "outputTokens",
            "output_tokens",
            "completionTokens",
            "completion_tokens",
        ],
    );
    let total = first_u64(obj, &["totalTokens", "total_tokens"]);
    if input.is_none() && output.is_none() && total.is_none() {
        return None;
    }
    let input = input.unwrap_or(0);
    let output = output.unwrap_or(0);
    let total = total.unwrap_or(input.saturating_add(output));
    if input == 0 && output == 0 && total == 0 {
        return None;
    }
    Some(WikiUsage {
        input_tokens: input,
        output_tokens: output,
        total_tokens: total,
    })
}

fn first_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(as_u64) {
            return Some(number);
        }
    }
    None
}

fn as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().filter(|n| *n >= 0).map(|n| n as u64))
        .or_else(|| {
            value
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| n as u64)
        })
}

pub fn run(
    cli: &Path,
    grok_home: &Path,
    model_id: &str,
    prompt: &str,
    cwd: &Path,
    timeout: Duration,
    max_turns: u32,
    on_started: impl FnOnce(),
    on_pulse: impl Fn(u64),
) -> Result<ParsedComplete, String> {
    if model_id.trim().is_empty() {
        return Err("wiki_model_missing".into());
    }
    if prompt.trim().is_empty() {
        return Err("wiki_empty_prompt".into());
    }
    let prompt_file = PromptFile::create(prompt)?;
    let mut cmd = Command::new(cli);
    cmd.args(headless_args(
        model_id,
        &prompt_file.path.display().to_string(),
        max_turns,
    ));
    cmd.env("GROK_HOME", grok_home);
    cmd.current_dir(cwd);
    crate::process_util::apply_no_window_std(&mut cmd);
    if let Some(path_env) = crate::process_util::enriched_path_env() {
        cmd.env("PATH", path_env);
    }
    crate::proxy::apply_to_std_command(&mut cmd);

    let started = std::time::Instant::now();
    let mut next_pulse = Duration::from_secs(2);
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("wiki_complete_spawn:{e}"))?;
    on_started();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("wiki_complete_timeout".into());
            }
            Ok(None) => {
                if started.elapsed() >= next_pulse {
                    on_pulse(started.elapsed().as_millis() as u64);
                    next_pulse += Duration::from_secs(2);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wiki_complete_spawn:{e}")),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("wiki_complete_spawn:{e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    finish_complete(output.status.success(), &stdout, &stderr, model_id)
}

/// Keep a page draft when the process stops on the turn cap after already
/// printing FILE blocks. A cap with no pages is `wiki_turn_cap`.
pub fn finish_complete(
    status_ok: bool,
    stdout: &str,
    stderr: &str,
    model_id: &str,
) -> Result<ParsedComplete, String> {
    let parsed = parse_complete_output(stdout);
    if is_model_rejection(&parsed.text) || is_model_rejection(stderr) {
        return Err(format!("wiki_model_unknown:{model_id}"));
    }
    if parsed.text.contains("---FILE:") {
        return Ok(parsed);
    }
    if is_turn_cap(&parsed.text) || is_turn_cap(stderr) {
        return Err("wiki_turn_cap".into());
    }
    if !status_ok {
        let preview = failure_preview(&parsed.text, stderr);
        return Err(format!("wiki_complete_failed:{preview}"));
    }
    if parsed.text.trim().is_empty() {
        let preview: String = stderr.chars().take(400).collect();
        return Err(format!("wiki_complete_empty:{preview}"));
    }
    Ok(parsed)
}

fn is_turn_cap(text: &str) -> bool {
    text.to_ascii_lowercase().contains("max turns reached")
}

/// Prompt body stays off the command line. Windows rejects argv past 32 KB.
struct PromptFile {
    path: std::path::PathBuf,
}

impl PromptFile {
    fn create(prompt: &str) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "atlas-wiki-prompt-{}.txt",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, prompt.as_bytes()).map_err(|e| format!("wiki_complete_spawn:{e}"))?;
        Ok(Self { path })
    }
}

impl Drop for PromptFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn is_model_rejection(text: &str) -> bool {
    let text = text.trim();
    text.contains("Couldn't set model") && text.contains("unknown model id")
}

fn failure_preview(stdout: &str, stderr: &str) -> String {
    let source = if stderr.trim().is_empty() { stdout } else { stderr };
    source.trim().chars().take(400).collect()
}

/// First home that actually lists `model_id`. Independent mode keeps enterprise
/// catalogs in the shared CLI home, while the picker also reads that home.
pub fn home_for_wiki_model(model_id: &str, session_data_mode: &str) -> std::path::PathBuf {
    let agent = crate::paths::resolve_agent_grok_home(session_data_mode);
    let mut homes = vec![agent.clone()];
    let shared = crate::paths::shared_cli_home();
    if shared != agent {
        homes.push(shared);
    }
    let legacy = crate::process_util::user_home().join(".grok");
    if homes.iter().all(|home| home != &legacy) {
        homes.push(legacy);
    }
    choose_wiki_home(model_id, &homes, home_lists_model)
        .map(|path| path.to_path_buf())
        .unwrap_or(agent)
}

pub fn choose_wiki_home<'a>(
    model_id: &str,
    homes: &'a [std::path::PathBuf],
    lists: impl Fn(&Path, &str) -> bool,
) -> Option<&'a Path> {
    let id = model_id.trim();
    if id.is_empty() {
        return None;
    }
    homes
        .iter()
        .find(|home| lists(home, id))
        .map(|home| home.as_path())
}

fn home_lists_model(home: &Path, model_id: &str) -> bool {
    if let Some(text) = fs_read(&home.join("config.toml")) {
        if crate::providers::parse_model_sections_for_proxy(&text)
            .iter()
            .any(|section| section.id == model_id)
        {
            return true;
        }
    }
    if let Some(text) = fs_read(&home.join("models_cache.json")) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if value
                .get("models")
                .and_then(|models| models.get(model_id))
                .is_some()
            {
                return true;
            }
        }
    }
    false
}

pub fn configured_model() -> Result<(String, std::path::PathBuf), String> {
    let settings = crate::store::load_settings();
    let agent = crate::paths::resolve_agent_grok_home(&settings.session_data_mode);
    let config = fs_read(&agent.join("config.toml")).unwrap_or_default();
    let model = match resolve_model_id(settings.model_id.as_deref(), &config) {
        Ok(id) => id,
        Err(_) => {
            let shared = fs_read(&crate::paths::shared_cli_home().join("config.toml"))
                .unwrap_or_default();
            resolve_model_id(None, &shared)?
        }
    };
    let home = home_for_wiki_model(&model, &settings.session_data_mode);
    Ok((model, home))
}

fn fs_read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_started_event_is_the_page_notice() {
        assert_eq!(WIKI_CLI_STARTED_EVENT, "wiki://cli-started");
        assert_eq!(WIKI_CLI_PULSE_EVENT, "wiki://cli-pulse");
    }

    #[test]
    fn turn_cap_keeps_file_blocks_and_rejects_an_empty_stop() {
        let kept = finish_complete(
            false,
            "---FILE: wiki/a.md---\nbody\n---END FILE---\nError: max turns reached",
            "Error: max turns reached",
            "arch-minimax",
        )
        .unwrap();
        assert!(kept.text.contains("wiki/a.md"));
        assert_eq!(
            finish_complete(false, "Error: max turns reached", "", "arch-minimax").unwrap_err(),
            "wiki_turn_cap"
        );
    }

    #[test]
    fn json_output_keeps_the_reply_and_the_token_counts() {
        let parsed = parse_complete_output(
            r#"{"result":"---FILE: wiki/a.md---\nbody\n---END FILE---","usage":{"input_tokens":12,"output_tokens":8,"total_tokens":20}}"#,
        );
        assert!(parsed.text.contains("wiki/a.md"));
        let usage = parsed.usage.expect("usage");
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 8);
        assert_eq!(usage.total_tokens, 20);
    }

    #[test]
    fn long_prompt_goes_to_a_file_instead_of_the_command_line() {
        let body = "知识库".repeat(40_000);
        let file = PromptFile::create(&body).unwrap();
        let args = headless_args("arch-minimax", &file.path.display().to_string(), 6);
        assert!(args.windows(2).any(|pair| {
            pair[0] == "--prompt-file" && pair[1] == file.path.display().to_string()
        }));
        assert!(!args.iter().any(|arg| arg.contains("知识库")));
        assert_eq!(std::fs::read_to_string(&file.path).unwrap(), body);
        let path = file.path.clone();
        drop(file);
        assert!(!path.exists());
    }

    #[test]
    fn plain_output_has_no_invented_usage() {
        let parsed = parse_complete_output("---FILE: wiki/a.md---\nbody\n---END FILE---");
        assert!(parsed.text.contains("wiki/a.md"));
        assert!(parsed.usage.is_none());
    }

    #[test]
    fn compile_home_uses_the_directory_that_lists_the_model() {
        let agent = std::env::temp_dir().join(format!("atlas-wiki-home-{}", uuid::Uuid::new_v4()));
        let shared = std::env::temp_dir().join(format!("atlas-wiki-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&agent).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(
            agent.join("config.toml"),
            "[model.kimi-self]\nmodel = \"kimi\"\n",
        )
        .unwrap();
        std::fs::write(
            shared.join("models_cache.json"),
            r#"{"models":{"arch-minimax":{"info":{"id":"arch-minimax"}}}}"#,
        )
        .unwrap();
        let homes = vec![agent.clone(), shared.clone()];
        assert_eq!(
            choose_wiki_home("arch-minimax", &homes, home_lists_model).unwrap(),
            shared.as_path()
        );
        assert_eq!(
            choose_wiki_home("kimi-self", &homes, home_lists_model).unwrap(),
            agent.as_path()
        );
        assert!(choose_wiki_home("missing", &homes, home_lists_model).is_none());
        let _ = std::fs::remove_dir_all(&agent);
        let _ = std::fs::remove_dir_all(&shared);
    }
}
