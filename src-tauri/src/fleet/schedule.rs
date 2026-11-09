//! Task status transitions. Pure: no sockets and no filesystem.

use serde::{Deserialize, Serialize};

pub const GRACE_MS: u64 = 15 * 60 * 1000;
pub const MAX_TERMINAL: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub agent_id: String,
    pub cwd: String,
    pub prompt: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    pub created_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpc_id: Option<String>,
    #[serde(default)]
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of: Option<String>,
    #[serde(default)]
    pub unreachable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreachable_at_ms: Option<u64>,
}

pub fn is_active(status: &str) -> bool {
    status == "claimed" || status == "running"
}

pub fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

/// A machine is busy only while a live run still owns it.
/// Unreachable runs wait out the grace period and do not block the next task.
pub fn agent_busy(tasks: &[TaskRecord], agent_id: &str) -> bool {
    tasks
        .iter()
        .any(|t| t.agent_id == agent_id && is_active(&t.status) && !t.unreachable)
}

pub fn claim_next(tasks: &mut [TaskRecord], agent_id: &str, now: u64) -> Option<usize> {
    if agent_busy(tasks, agent_id) {
        return None;
    }
    let mut best: Option<usize> = None;
    for (i, task) in tasks.iter().enumerate() {
        if task.agent_id == agent_id && task.status == "queued" {
            let older = best
                .map(|j| task.created_at_ms < tasks[j].created_at_ms)
                .unwrap_or(true);
            if older {
                best = Some(i);
            }
        }
    }
    let i = best?;
    tasks[i].status = "claimed".into();
    tasks[i].started_at_ms = Some(now);
    tasks[i].unreachable = false;
    tasks[i].unreachable_at_ms = None;
    Some(i)
}

pub fn mark_running(task: &mut TaskRecord, session_id: &str, rpc_id: &str) {
    if task.status == "claimed" {
        task.status = "running".into();
    }
    if !session_id.is_empty() {
        task.session_id = session_id.to_string();
    }
    task.rpc_id = Some(rpc_id.to_string());
}

pub fn mark_agent_unreachable(tasks: &mut [TaskRecord], agent_id: &str, now: u64) {
    for task in tasks {
        if task.agent_id == agent_id && is_active(&task.status) && !task.unreachable {
            task.unreachable = true;
            task.unreachable_at_ms = Some(now);
        }
    }
}

/// After a leader restart, in-flight tasks are unreachable until the grace ends.
/// Queued tasks stay queued.
pub fn mark_restored_unreachable(tasks: &mut [TaskRecord], now: u64) {
    for task in tasks {
        if is_active(&task.status) && !task.unreachable {
            task.unreachable = true;
            task.unreachable_at_ms = Some(now);
        }
    }
}

/// Fail unreachable runs after the grace and enqueue one retry for a first failure.
pub fn expire_unreachable(tasks: &mut Vec<TaskRecord>, now: u64, grace_ms: u64) -> Vec<String> {
    let mut sources = Vec::new();
    for task in tasks.iter_mut() {
        if !task.unreachable || !is_active(&task.status) {
            continue;
        }
        let at = task.unreachable_at_ms.unwrap_or(now);
        if now.saturating_sub(at) < grace_ms {
            continue;
        }
        task.status = "failed".into();
        task.failure_reason = Some("runtime_offline".into());
        task.error = Some("runtime_offline".into());
        task.unreachable = false;
        task.finished_at_ms = Some(now);
        if task.retry_of.is_none() {
            sources.push(task.clone());
        }
    }
    let mut created = Vec::new();
    for src in sources {
        if tasks
            .iter()
            .any(|t| t.retry_of.as_deref() == Some(src.id.as_str()))
        {
            continue;
        }
        let id = format!("retry-{}-{now}", src.id);
        tasks.push(TaskRecord {
            id: id.clone(),
            agent_id: src.agent_id,
            cwd: src.cwd,
            prompt: src.prompt,
            status: "queued".into(),
            error: None,
            failure_reason: None,
            created_at_ms: now,
            started_at_ms: None,
            finished_at_ms: None,
            rpc_id: None,
            session_id: String::new(),
            retry_of: Some(src.id),
            unreachable: false,
            unreachable_at_ms: None,
        });
        created.push(id);
    }
    created
}

/// Drop the oldest finished tasks beyond `keep`. Active and queued tasks stay.
pub fn prune_terminal(tasks: &mut Vec<TaskRecord>, keep: usize) -> Vec<String> {
    let mut terminal: Vec<usize> = tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| is_terminal(&t.status))
        .map(|(i, _)| i)
        .collect();
    terminal.sort_by(|&a, &b| {
        let ta = tasks[a].finished_at_ms.unwrap_or(tasks[a].created_at_ms);
        let tb = tasks[b].finished_at_ms.unwrap_or(tasks[b].created_at_ms);
        tb.cmp(&ta)
    });
    if terminal.len() <= keep {
        return Vec::new();
    }
    let mut drop_idx: Vec<usize> = terminal.into_iter().skip(keep).collect();
    drop_idx.sort_unstable_by(|a, b| b.cmp(a));
    let mut ids = Vec::new();
    for i in drop_idx {
        ids.push(tasks.remove(i).id);
    }
    ids
}

pub fn new_task(id: &str, agent_id: &str, cwd: &str, prompt: &str, now: u64) -> TaskRecord {
    TaskRecord {
        id: id.to_string(),
        agent_id: agent_id.to_string(),
        cwd: cwd.to_string(),
        prompt: prompt.to_string(),
        status: "queued".into(),
        error: None,
        failure_reason: None,
        created_at_ms: now,
        started_at_ms: None,
        finished_at_ms: None,
        rpc_id: None,
        session_id: String::new(),
        retry_of: None,
        unreachable: false,
        unreachable_at_ms: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(id: &str, agent: &str, at: u64) -> TaskRecord {
        new_task(id, agent, "/data/proj", "do the thing", at)
    }

    #[test]
    fn claim_is_fifo_and_one_active_per_machine() {
        let mut tasks = vec![queued("b", "box", 20), queued("a", "box", 10)];
        let i = claim_next(&mut tasks, "box", 100).unwrap();
        assert_eq!(tasks[i].id, "a");
        assert_eq!(tasks[i].status, "claimed");
        assert!(claim_next(&mut tasks, "box", 101).is_none());
        tasks[i].unreachable = true;
        let j = claim_next(&mut tasks, "box", 102).unwrap();
        assert_eq!(tasks[j].id, "b");
    }

    #[test]
    fn grace_fails_once_and_retries_once() {
        let mut tasks = vec![queued("t1", "box", 1)];
        claim_next(&mut tasks, "box", 2);
        mark_agent_unreachable(&mut tasks, "box", 1_000);
        assert!(expire_unreachable(&mut tasks, 1_000 + GRACE_MS - 1, GRACE_MS).is_empty());
        assert_eq!(tasks[0].status, "claimed");
        let created = expire_unreachable(&mut tasks, 1_000 + GRACE_MS, GRACE_MS);
        assert_eq!(tasks[0].status, "failed");
        assert_eq!(tasks[0].failure_reason.as_deref(), Some("runtime_offline"));
        assert_eq!(created.len(), 1);
        assert_eq!(tasks[1].status, "queued");
        assert_eq!(tasks[1].retry_of.as_deref(), Some("t1"));
        claim_next(&mut tasks, "box", 9_000);
        mark_agent_unreachable(&mut tasks, "box", 9_000);
        let again = expire_unreachable(&mut tasks, 9_000 + GRACE_MS, GRACE_MS);
        assert!(again.is_empty());
        assert_eq!(
            tasks.iter().filter(|t| t.status == "queued").count(),
            0
        );
    }

    #[test]
    fn restore_leaves_queued_and_marks_running() {
        let mut tasks = vec![queued("q", "box", 1)];
        tasks.push(queued("r", "box", 2));
        tasks[1].status = "running".into();
        mark_restored_unreachable(&mut tasks, 50);
        assert_eq!(tasks[0].status, "queued");
        assert!(!tasks[0].unreachable);
        assert!(tasks[1].unreachable);
        assert_eq!(tasks[1].status, "running");
    }

    #[test]
    fn prune_keeps_two_hundred_finished_and_all_open() {
        let mut tasks = vec![queued("open", "local", 1)];
        for i in 0..MAX_TERMINAL + 1 {
            let mut t = queued(&format!("d{i}"), "local", i as u64);
            t.status = "completed".into();
            t.finished_at_ms = Some(i as u64);
            tasks.push(t);
        }
        let dropped = prune_terminal(&mut tasks, MAX_TERMINAL);
        assert_eq!(dropped.len(), 1);
        assert!(dropped.contains(&"d0".to_string()));
        assert!(tasks.iter().any(|t| t.id == "open"));
        assert_eq!(
            tasks.iter().filter(|t| t.status == "completed").count(),
            MAX_TERMINAL
        );
    }
}
