//! Fleet task files under the app data directory.
//!
//! `{dir}/{id}.json` is the dispatch record. `{dir}/{id}.steps.jsonl` is the
//! folded execution log. A torn last line is dropped on read.

use std::fs;
use std::path::{Path, PathBuf};

use super::protocol::Step;
use super::schedule::TaskRecord;

pub fn tasks_dir() -> PathBuf {
    let dir = crate::paths::app_data_root().join("fleet").join("tasks");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn task_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn steps_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.steps.jsonl"))
}

pub fn save_task(dir: &Path, task: &TaskRecord) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(task).map_err(|e| e.to_string())?;
    crate::store_lock::write_bytes_atomic(&task_path(dir, &task.id), &bytes)
}

pub fn write_steps(dir: &Path, id: &str, steps: &[Step]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut body = String::new();
    for step in steps {
        body.push_str(&serde_json::to_string(step).map_err(|e| e.to_string())?);
        body.push('\n');
    }
    crate::store_lock::write_bytes_atomic(&steps_path(dir, id), body.as_bytes())
}

pub fn load_tasks(dir: &Path) -> Vec<TaskRecord> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.ends_with(".json") || name.ends_with(".steps.json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(task) = serde_json::from_str::<TaskRecord>(&text) {
            out.push(task);
        }
    }
    out.sort_by(|a, b| a.created_at_ms.cmp(&b.created_at_ms));
    out
}

pub fn load_steps(dir: &Path, id: &str) -> Vec<Step> {
    let Ok(text) = fs::read_to_string(steps_path(dir, id)) else {
        return Vec::new();
    };
    let mut lines: Vec<&str> = text.lines().collect();
    if let Some(last) = lines.last() {
        if serde_json::from_str::<Step>(last).is_err() {
            lines.pop();
        }
    }
    lines
        .into_iter()
        .filter_map(|line| serde_json::from_str::<Step>(line).ok())
        .collect()
}

pub fn delete_task_files(dir: &Path, id: &str) {
    let _ = fs::remove_file(task_path(dir, id));
    let _ = fs::remove_file(steps_path(dir, id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet::schedule::new_task;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas-fleet-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_task_and_drop_torn_step_line() {
        let dir = scratch();
        let task = new_task("t1", "local", "/data/proj", "hello", 10);
        save_task(&dir, &task).unwrap();
        let loaded = load_tasks(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].prompt, "hello");

        let good = Step {
            kind: "message".into(),
            text: "ok".into(),
            tool_call_id: None,
            tool_name: None,
            title: None,
            status: None,
        };
        write_steps(&dir, "t1", &[good.clone()]).unwrap();
        let path = dir.join("t1.steps.jsonl");
        let mut raw = fs::read_to_string(&path).unwrap();
        raw.push_str("{\"kind\":\"thought\",\"text\":");
        fs::write(&path, raw).unwrap();
        let steps = load_steps(&dir, "t1");
        assert_eq!(steps, vec![good]);
        let _ = fs::remove_dir_all(&dir);
    }
}
