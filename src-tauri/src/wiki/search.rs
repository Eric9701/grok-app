//! Keyword search over `wiki/*.md`. No vector index.

use std::fs;
use std::path::Path;

use serde::Serialize;

use super::path::{read_text, require_project};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiHit {
    pub path: String,
    pub title: String,
    pub score: i32,
    pub snippet: String,
}

pub fn search(project: &Path, query: &str, limit: usize) -> Result<Vec<WikiHit>, String> {
    let root = require_project(project)?;
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Err("wiki_empty_query".into());
    }
    let mut hits = Vec::new();
    let mut pages: Vec<String> = Vec::new();
    collect_pages(&root.join("wiki"), &root.join("wiki"), &mut pages)?;
    for rel in pages {
        let full = format!("wiki/{rel}");
        let content = read_text(&root, &full).unwrap_or_default();
        let title = title_of(&content, &rel);
        let mut score = 0;
        let mut first: Option<usize> = None;
        let lower = content.to_lowercase();
        let title_l = title.to_lowercase();
        for token in &tokens {
            if title_l.contains(token) {
                score += 10;
            }
            if let Some(at) = lower.find(token) {
                score += 1;
                if first.is_none() {
                    first = Some(at);
                }
            }
        }
        if score == 0 {
            continue;
        }
        hits.push(WikiHit {
            path: full,
            title,
            score,
            snippet: snippet(&content, first.unwrap_or(0)),
        });
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.path.cmp(&b.path)));
    hits.truncate(limit.clamp(1, 50));
    Ok(hits)
}

fn collect_pages(dir: &Path, wiki: &Path, out: &mut Vec<String>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| format!("wiki_io:{e}"))? {
        let path = entry.map_err(|e| format!("wiki_io:{e}"))?.path();
        if path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        if path.is_dir() {
            collect_pages(&path, wiki, out)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let rel = path
            .strip_prefix(wiki)
            .map_err(|_| "wiki_path".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.split('/').any(|s| s.starts_with('.')) {
            continue;
        }
        out.push(rel);
    }
    Ok(())
}

fn title_of(content: &str, rel: &str) -> String {
    for line in content.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("# ") {
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    rel.rsplit('/')
        .next()
        .unwrap_or(rel)
        .trim_end_matches(".md")
        .to_string()
}

fn snippet(content: &str, at: usize) -> String {
    let start = at.saturating_sub(40);
    let end = (at + 120).min(content.len());
    let mut start = floor_char(content, start);
    let end = floor_char(content, end);
    if start > end {
        start = 0;
    }
    content[start..end].replace('\n', " ")
}

fn floor_char(text: &str, mut i: usize) -> usize {
    if i >= text.len() {
        return text.len();
    }
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn tokenize(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut cjk = String::new();
    let flush_word = |word: &mut String, out: &mut Vec<String>| {
        if word.chars().count() >= 2 {
            out.push(word.to_lowercase());
        }
        word.clear();
    };
    let flush_cjk = |cjk: &mut String, out: &mut Vec<String>| {
        let chars: Vec<char> = cjk.chars().collect();
        if chars.len() == 1 {
            out.push(chars[0].to_string());
        } else {
            for pair in chars.windows(2) {
                out.push(pair.iter().collect());
            }
        }
        cjk.clear();
    };
    for ch in query.chars() {
        if ch.is_ascii_alphanumeric() {
            flush_cjk(&mut cjk, &mut out);
            word.push(ch);
        } else if is_cjk(ch) {
            flush_word(&mut word, &mut out);
            cjk.push(ch);
        } else {
            flush_word(&mut word, &mut out);
            flush_cjk(&mut cjk, &mut out);
        }
    }
    flush_word(&mut word, &mut out);
    flush_cjk(&mut cjk, &mut out);
    out
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'
            | '\u{3400}'..='\u{4DBF}'
    )
}
