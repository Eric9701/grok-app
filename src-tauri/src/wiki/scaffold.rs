//! Create a knowledge-base folder from a template, and copy raw sources into it.
//!
//! Existing files are left in place. Nothing here writes `~/.atlas` or `config.toml`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::path::require_project;

const COPY_CAP: usize = 40;
const COPY_BYTES: u64 = 2_000_000;
const URL_BYTES: usize = 200_000;

pub fn scaffold(parent: &Path, name: &str, template: &str) -> Result<PathBuf, String> {
    if !parent.is_absolute() {
        return Err("wiki_project".into());
    }
    let parent = parent
        .canonicalize()
        .map_err(|_| "wiki_project".to_string())?;
    if !parent.is_dir() {
        return Err("wiki_project".into());
    }
    let folder = safe_name(name)?;
    let dest = parent.join(&folder);
    if dest.exists() && !dest.is_dir() {
        return Err("wiki_name".into());
    }
    if dest.is_symlink() {
        return Err("wiki_symlink".into());
    }
    fs::create_dir_all(&dest).map_err(|e| format!("wiki_io:{e}"))?;
    let dest = dest.canonicalize().map_err(|e| format!("wiki_io:{e}"))?;
    if !dest.starts_with(&parent) {
        return Err("wiki_path".into());
    }
    apply_template(&dest, template, false)?;
    Ok(dest)
}

/// Wipe an existing knowledge base and apply a template from scratch.
/// Deletes `wiki/`, `raw/`, and `.atlas-wiki/` when they are real directories
/// inside the project. The project folder itself stays.
pub fn init_existing(project: &Path, template: &str) -> Result<PathBuf, String> {
    if !project.is_absolute() {
        return Err("wiki_project".into());
    }
    if project
        .symlink_metadata()
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("wiki_symlink".into());
    }
    let dest = project
        .canonicalize()
        .map_err(|_| "wiki_project".to_string())?;
    if !dest.is_dir() {
        return Err("wiki_project".into());
    }
    // Refuse an unknown type before deleting anything.
    if template_spec(template).is_none() {
        return Err("wiki_template".into());
    }
    for name in ["wiki", "raw", ".atlas-wiki"] {
        remove_child(&dest, name)?;
    }
    apply_template(&dest, template, false)?;
    Ok(dest)
}

fn remove_child(root: &Path, name: &str) -> Result<(), String> {
    let path = root.join(name);
    if path.symlink_metadata().is_err() {
        return Ok(());
    }
    remove_tree(root, &path)
}

fn remove_tree(root: &Path, path: &Path) -> Result<(), String> {
    let meta = path
        .symlink_metadata()
        .map_err(|e| format!("wiki_io:{e}"))?;
    if meta.file_type().is_symlink() {
        return Err("wiki_symlink".into());
    }
    let canon = path.canonicalize().map_err(|e| format!("wiki_io:{e}"))?;
    if !canon.starts_with(root) {
        return Err("wiki_path".into());
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| format!("wiki_io:{e}"))? {
            let entry = entry.map_err(|e| format!("wiki_io:{e}"))?;
            remove_tree(root, &entry.path())?;
        }
        fs::remove_dir(path).map_err(|e| format!("wiki_io:{e}"))?;
    } else {
        fs::remove_file(path).map_err(|e| format!("wiki_io:{e}"))?;
    }
    Ok(())
}

fn apply_template(dest: &Path, template: &str, refresh_schema: bool) -> Result<(), String> {
    let spec = template_spec(template).ok_or_else(|| "wiki_template".to_string())?;
    for dir in spec.dirs {
        fs::create_dir_all(dest.join(dir)).map_err(|e| format!("wiki_io:{e}"))?;
    }
    for (rel, body) in spec.files {
        let path = dest.join(rel);
        if path.exists() && !(refresh_schema && *rel == "wiki/schema.md") {
            continue;
        }
        if let Some(parent_dir) = path.parent() {
            fs::create_dir_all(parent_dir).map_err(|e| format!("wiki_io:{e}"))?;
        }
        fs::write(&path, body).map_err(|e| format!("wiki_io:{e}"))?;
    }
    Ok(())
}

pub fn import_files(project: &Path, sources: &[String]) -> Result<Vec<String>, String> {
    let root = require_project(project)?;
    let dest_dir = root.join("raw").join("sources");
    fs::create_dir_all(&dest_dir).map_err(|e| format!("wiki_io:{e}"))?;
    let mut written = Vec::new();
    for source in sources {
        if written.len() >= COPY_CAP {
            break;
        }
        let path = PathBuf::from(source.trim());
        if !path.is_absolute() {
            continue;
        }
        collect_copy(&path, &dest_dir, &mut written)?;
    }
    if written.is_empty() {
        return Err("wiki_import_empty".into());
    }
    Ok(written)
}

pub fn import_url(project: &Path, url: &str) -> Result<String, String> {
    let target = parse_public_url(url)?;
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("wiki_import_url:{e}"))?;
    let response = client
        .get(target.clone())
        .send()
        .map_err(|e| format!("wiki_import_url:{e}"))?;
    if !response.status().is_success() {
        return Err(format!("wiki_import_url:{}", response.status()));
    }
    let bytes = response
        .bytes()
        .map_err(|e| format!("wiki_import_url:{e}"))?;
    let slice = &bytes[..bytes.len().min(URL_BYTES)];
    let text = String::from_utf8_lossy(slice);
    let plain = strip_tags(&text);
    if plain.trim().is_empty() {
        return Err("wiki_import_empty".into());
    }
    let title = target
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .filter(|part| !part.is_empty())
        .unwrap_or("page");
    let body = format!("# {title}\n\n来源：{url}\n\n{plain}\n");
    let mut written = Vec::new();
    let root = require_project(project)?;
    let dest_dir = root.join("raw").join("sources");
    fs::create_dir_all(&dest_dir).map_err(|e| format!("wiki_io:{e}"))?;
    let dest = unique_dest(&dest_dir, &format!("{title}.md"))?;
    fs::write(&dest, body).map_err(|e| format!("wiki_io:{e}"))?;
    written.push(format!(
        "raw/sources/{}",
        dest.file_name().and_then(|n| n.to_str()).unwrap_or("page.md")
    ));
    Ok(written[0].clone())
}

pub fn remove_raw(project: &Path, rel: &str) -> Result<(), String> {
    let rel = super::path::normalize_rel(rel)?;
    if !rel.starts_with("raw/") || rel.ends_with('/') {
        return Err("wiki_path".into());
    }
    let dest = super::path::resolve_inside(project, &rel)?;
    if dest.is_file() {
        fs::remove_file(&dest).map_err(|e| format!("wiki_io:{e}"))?;
    }
    Ok(())
}

fn collect_copy(path: &Path, dest_dir: &Path, written: &mut Vec<String>) -> Result<(), String> {
    if written.len() >= COPY_CAP || !path.exists() {
        return Ok(());
    }
    if path
        .symlink_metadata()
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(true)
    {
        return Ok(());
    }
    if path.is_dir() {
        let entries = fs::read_dir(path).map_err(|e| format!("wiki_io:{e}"))?;
        for entry in entries {
            if written.len() >= COPY_CAP {
                break;
            }
            let entry = entry.map_err(|e| format!("wiki_io:{e}"))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            collect_copy(&entry.path(), dest_dir, written)?;
        }
        return Ok(());
    }
    if !path.is_file() || !text_extension(path) {
        return Ok(());
    }
    let meta = fs::metadata(path).map_err(|e| format!("wiki_io:{e}"))?;
    if meta.len() > COPY_BYTES {
        return Ok(());
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "wiki_name".to_string())?;
    let dest = unique_dest(dest_dir, name)?;
    if let Ok(src) = path.canonicalize() {
        if dest.exists() && dest.canonicalize().ok().as_ref() == Some(&src) {
            return Ok(());
        }
    }
    fs::copy(path, &dest).map_err(|e| format!("wiki_io:{e}"))?;
    let file_name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("source.md");
    written.push(format!("raw/sources/{file_name}"));
    Ok(())
}

fn unique_dest(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let (stem, ext) = split_name(name);
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
        if n > 100 {
            return Err("wiki_import_empty".into());
        }
    }
    Ok(candidate)
}

fn split_name(name: &str) -> (String, String) {
    let clean = safe_file_name(name);
    match clean.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && text_ext(ext) => (stem.to_string(), ext.to_string()),
        _ => (clean, "md".into()),
    }
}

fn safe_file_name(name: &str) -> String {
    let tail = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("source")
        .trim();
    let mut out = String::new();
    for ch in tail.chars() {
        if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
            out.push('-');
        } else {
            out.push(ch);
        }
    }
    let out = out.trim_matches('.').trim().to_string();
    if out.is_empty() || out.starts_with('.') {
        "source.md".into()
    } else {
        out.chars().take(80).collect()
    }
}

fn text_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(text_ext)
        .unwrap_or(false)
}

fn text_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "md" | "mdx" | "txt" | "csv" | "json" | "yaml" | "yml" | "xml" | "html" | "htm" | "log"
    )
}

fn safe_name(name: &str) -> Result<String, String> {
    let text = name.trim();
    if text.is_empty()
        || text.contains('/')
        || text.contains('\\')
        || text.contains("..")
        || text.starts_with('.')
        || text.chars().any(|ch| ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err("wiki_name".into());
    }
    let trimmed: String = text.chars().take(80).collect();
    if trimmed.is_empty() {
        return Err("wiki_name".into());
    }
    Ok(trimmed)
}

fn parse_public_url(raw: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "wiki_import_url".to_string())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("wiki_import_url".into());
    }
    let host = url.host_str().unwrap_or("").trim().to_ascii_lowercase();
    if host.is_empty() || host_blocked(&host) {
        return Err("wiki_import_url".into());
    }
    Ok(url)
}

fn host_blocked(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".local") || host.ends_with(".localhost") {
        return true;
    }
    let Ok(ip) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified() || is_v6_local(&v6)
        }
    }
}

fn is_v6_local(ip: &std::net::Ipv6Addr) -> bool {
    let segments = ip.segments();
    let head = segments[0];
    (head & 0xfe00) == 0xfc00 || (head & 0xffc0) == 0xfe80
}

fn strip_tags(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut skip = false;
    let mut out = String::new();
    let mut chars = input.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if !skip && (lower[index..].starts_with("<script") || lower[index..].starts_with("<style")) {
            skip = true;
        }
        if skip {
            if lower[index..].starts_with("</script>") || lower[index..].starts_with("</style>") {
                skip = false;
            }
            continue;
        }
        if ch == '<' {
            while let Some((_, next)) = chars.peek() {
                if *next == '>' {
                    chars.next();
                    break;
                }
                chars.next();
            }
            out.push(' ');
            continue;
        }
        out.push(ch);
    }
    let mut collapsed = String::new();
    let mut space = false;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !space && !collapsed.is_empty() {
                collapsed.push(' ');
            }
            space = true;
        } else {
            space = false;
            collapsed.push(ch);
        }
    }
    collapsed.trim().to_string()
}

struct Spec {
    dirs: &'static [&'static str],
    files: &'static [(&'static str, &'static str)],
}

fn template_spec(id: &str) -> Option<Spec> {
    Some(match id {
        "general" => Spec {
            dirs: &["raw/assets", "raw/sources", "wiki/concepts", "wiki/sources"],
            files: COMMON_FILES,
        },
        "research" => Spec {
            dirs: &[
                "raw/assets",
                "raw/sources",
                "wiki/entities",
                "wiki/concepts",
                "wiki/sources",
                "wiki/queries",
                "wiki/synthesis",
                "wiki/comparisons",
                "wiki/thesis",
            ],
            files: RESEARCH_FILES,
        },
        "reading" => Spec {
            dirs: &[
                "raw/assets",
                "raw/sources",
                "wiki/characters",
                "wiki/themes",
                "wiki/plot",
                "wiki/concepts",
                "wiki/sources",
            ],
            files: READING_FILES,
        },
        "personal" => Spec {
            dirs: &[
                "raw/assets",
                "raw/sources",
                "wiki/goals",
                "wiki/habits",
                "wiki/reflections",
                "wiki/concepts",
            ],
            files: PERSONAL_FILES,
        },
        "business" => Spec {
            dirs: &[
                "raw/assets",
                "raw/sources",
                "wiki/meetings",
                "wiki/decisions",
                "wiki/projects",
                "wiki/stakeholders",
                "wiki/entities",
                "wiki/concepts",
            ],
            files: BUSINESS_FILES,
        },
        _ => return None,
    })
}

const COMMON_FILES: &[(&str, &str)] = &[
    (
        "wiki/purpose.md",
        "# 目的\n\n这个知识库用来整理原始资料。原文放在 `raw/sources/`，整理后的页面放在 `wiki/`。\n",
    ),
    (
        "wiki/index.md",
        "# 目录\n\n还没有页面。\n",
    ),
    (
        "wiki/log.md",
        "# 日志\n",
    ),
    (
        "wiki/overview.md",
        "# 概览\n\n还没有概览。\n",
    ),
    (
        "wiki/schema.md",
        "# Schema\n\n## Page Types\n\n| type | directory |\n|------|-----------|\n| concept | wiki/concepts/ |\n| source | wiki/sources/ |\n| overview | wiki/ |\n",
    ),
];

const RESEARCH_FILES: &[(&str, &str)] = &[
    COMMON_FILES[0],
    COMMON_FILES[1],
    COMMON_FILES[2],
    COMMON_FILES[3],
    RESEARCH_SCHEMA,
];
const READING_FILES: &[(&str, &str)] = &[
    COMMON_FILES[0],
    COMMON_FILES[1],
    COMMON_FILES[2],
    COMMON_FILES[3],
    READING_SCHEMA,
];
const PERSONAL_FILES: &[(&str, &str)] = &[
    COMMON_FILES[0],
    COMMON_FILES[1],
    COMMON_FILES[2],
    COMMON_FILES[3],
    PERSONAL_SCHEMA,
];
const BUSINESS_FILES: &[(&str, &str)] = &[
    COMMON_FILES[0],
    COMMON_FILES[1],
    COMMON_FILES[2],
    COMMON_FILES[3],
    BUSINESS_SCHEMA,
];

const RESEARCH_SCHEMA: (&str, &str) = (
    "wiki/schema.md",
    "# Schema\n\n## Page Types\n\n| type | directory |\n|------|-----------|\n| entity | wiki/entities/ |\n| concept | wiki/concepts/ |\n| source | wiki/sources/ |\n| query | wiki/queries/ |\n| comparison | wiki/comparisons/ |\n| synthesis | wiki/synthesis/ |\n| thesis | wiki/thesis/ |\n| overview | wiki/ |\n",
);

const READING_SCHEMA: (&str, &str) = (
    "wiki/schema.md",
    "# Schema\n\n## Page Types\n\n| type | directory |\n|------|-----------|\n| character | wiki/characters/ |\n| theme | wiki/themes/ |\n| plot | wiki/plot/ |\n| concept | wiki/concepts/ |\n| source | wiki/sources/ |\n| overview | wiki/ |\n",
);

const PERSONAL_SCHEMA: (&str, &str) = (
    "wiki/schema.md",
    "# Schema\n\n## Page Types\n\n| type | directory |\n|------|-----------|\n| goal | wiki/goals/ |\n| habit | wiki/habits/ |\n| reflection | wiki/reflections/ |\n| concept | wiki/concepts/ |\n| overview | wiki/ |\n",
);

const BUSINESS_SCHEMA: (&str, &str) = (
    "wiki/schema.md",
    "# Schema\n\n## Page Types\n\n| type | directory |\n|------|-----------|\n| meeting | wiki/meetings/ |\n| decision | wiki/decisions/ |\n| project | wiki/projects/ |\n| stakeholder | wiki/stakeholders/ |\n| entity | wiki/entities/ |\n| concept | wiki/concepts/ |\n| overview | wiki/ |\n",
);

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atlas-wiki-scaffold-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn research_template_creates_folders_and_keeps_an_existing_page() {
        let parent = scratch();
        let created = scaffold(&parent, "notes", "research").unwrap();
        assert!(created.join("wiki/thesis").is_dir());
        assert!(created.join("raw/sources").is_dir());
        assert!(created.join("wiki/schema.md").is_file());
        fs::write(created.join("wiki/purpose.md"), "keep").unwrap();
        scaffold(&parent, "notes", "research").unwrap();
        assert_eq!(fs::read_to_string(created.join("wiki/purpose.md")).unwrap(), "keep");
        assert!(scaffold(&parent, "../escape", "research").is_err());
        assert!(scaffold(&parent, "notes", "nope").is_err());
        let _ = fs::remove_dir_all(&parent);
    }

    #[test]
    fn init_existing_clears_pages_and_sources_then_applies_the_type() {
        let parent = scratch();
        let created = scaffold(&parent, "notes", "general").unwrap();
        fs::write(created.join("wiki/purpose.md"), "keep").unwrap();
        fs::create_dir_all(created.join("wiki/concepts")).unwrap();
        fs::write(created.join("wiki/concepts/pump.md"), "page").unwrap();
        fs::write(created.join("raw/sources/clip.md"), "source").unwrap();
        fs::create_dir_all(created.join(".atlas-wiki")).unwrap();
        fs::write(created.join(".atlas-wiki/usage.json"), "[]").unwrap();
        assert!(init_existing(&created, "nope").is_err());
        assert_eq!(
            fs::read_to_string(created.join("wiki/purpose.md")).unwrap(),
            "keep"
        );
        init_existing(&created, "research").unwrap();
        assert!(!created.join("wiki/concepts/pump.md").exists());
        assert!(!created.join("raw/sources/clip.md").exists());
        assert!(!created.join(".atlas-wiki/usage.json").exists());
        assert!(created.join("wiki/thesis").is_dir());
        assert!(created.join("raw/sources").is_dir());
        let purpose = fs::read_to_string(created.join("wiki/purpose.md")).unwrap();
        assert!(purpose.contains("目的"));
        let schema = fs::read_to_string(created.join("wiki/schema.md")).unwrap();
        assert!(schema.contains("wiki/thesis/"));
        let _ = fs::remove_dir_all(&parent);
    }

    #[test]
    fn import_copies_a_text_file_and_skips_a_second_copy_name() {
        let parent = scratch();
        let project = scaffold(&parent, "notes", "general").unwrap();
        let src = parent.join("clip.md");
        fs::write(&src, "hello").unwrap();
        let first = import_files(&project, &[src.display().to_string()]).unwrap();
        assert_eq!(first, vec!["raw/sources/clip.md".to_string()]);
        let second = import_files(&project, &[src.display().to_string()]).unwrap();
        assert_eq!(second, vec!["raw/sources/clip-2.md".to_string()]);
        remove_raw(&project, "raw/sources/clip.md").unwrap();
        assert!(!project.join("raw/sources/clip.md").exists());
        assert!(remove_raw(&project, "wiki/purpose.md").is_err());
        assert!(host_blocked("127.0.0.1"));
        assert!(host_blocked("localhost"));
        assert!(!host_blocked("example.com"));
        let _ = fs::remove_dir_all(&parent);
    }
}
