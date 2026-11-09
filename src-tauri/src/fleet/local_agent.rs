//! Local tasks: one `atlas agent --always-approve stdio` process per task.
//! The process does not dial the fleet WebSocket.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::protocol::{inbound_frame_to_line, LineEvent};

pub struct LocalPipe {
    pub child: Child,
    pub out_tx: UnboundedSender<String>,
    pub line_rx: UnboundedReceiver<LineEvent>,
}

pub async fn open() -> Result<LocalPipe, String> {
    let settings = crate::store::load_settings();
    let probe = crate::cli_probe::probe_cli(settings.manual_cli_path.as_deref());
    let cli_path = probe
        .path
        .filter(|_| probe.found)
        .ok_or_else(|| "runtime_missing_executable".to_string())?;
    let home: PathBuf = crate::paths::resolve_agent_grok_home(&settings.session_data_mode);
    let mut cmd = Command::new(&cli_path);
    cmd.arg("agent").arg("--always-approve").arg("stdio");
    crate::process_util::apply_process_group_no_window_tokio(&mut cmd);
    crate::process_util::apply_cli_env_tokio(&mut cmd);
    cmd.env("GROK_HOME", &home);
    cmd.env("GROK_CLAUDE_MCPS_ENABLED", "false");
    cmd.env("GROK_CURSOR_MCPS_ENABLED", "false");
    crate::proxy::apply_to_tokio_command(&mut cmd);
    if home.is_dir() {
        cmd.current_dir(&home);
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|_| "runtime_missing_executable".to_string())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "runtime_missing_executable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "runtime_missing_executable".to_string())?;
    let stderr = child.stderr.take();

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        let mut stdin = stdin;
        while let Some(line) = out_rx.recv().await {
            if stdin.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if stdin.write_all(b"\n").await.is_err() {
                break;
            }
            if stdin.flush().await.is_err() {
                break;
            }
        }
    });

    let (line_tx, line_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout);
        let mut buf = String::new();
        loop {
            buf.clear();
            match lines.read_line(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Some(line) = inbound_frame_to_line(&buf) {
                        if line_tx.send(LineEvent::Line(line)).is_err() {
                            break;
                        }
                    }
                }
            }
        }
        let _ = line_tx.send(LineEvent::Closed);
    });
    if let Some(stderr) = stderr {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr);
            let mut buf = String::new();
            loop {
                buf.clear();
                match lines.read_line(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let t = buf.trim();
                        if !t.is_empty() {
                            tracing::debug!(target: "fleet", "local cli: {t}");
                        }
                    }
                }
            }
        });
    }
    Ok(LocalPipe {
        child,
        out_tx,
        line_rx,
    })
}

pub async fn shutdown(pipe: LocalPipe) {
    drop(pipe.out_tx);
    if let Some(pid) = pipe.child.id() {
        crate::process_util::kill_process_tree_async(pid).await;
    }
    let mut child = pipe.child;
    let _ = child.start_kill();
    let _ = child.wait().await;
}
