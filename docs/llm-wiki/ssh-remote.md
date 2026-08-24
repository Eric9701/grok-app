# SSH remote hosts

Remote workspaces are sliced. This page is the product contract for agents.

Related: [settings-ia.md](./settings-ia.md), [session-continuity.md](./session-continuity.md), [media-delivery.md](./media-delivery.md).

## Waves

| Wave | Ships | Does not |
|------|--------|----------|
| **1** | Settings → Runtime → **SSH**: list `~/.ssh/config` Host aliases, **Test connection**, probe remote `grok` + `~/.grok/auth.json`. Copy-paste install / `grok login --device-auth` when missing. | Open a remote folder, file tree, edit, spawn remote agent, tmux |
| **1b** | Live search, Watching / Available lists, watch switch (ControlMaster), scan remote Grok sessions into the sidebar, new-session Remote chip + path picker | Spawn `grok` on the host; in-app file tree/edit of remote files |
| **2** | Remote path as a Project; sidebar tree; markdown preview; text save | Agent cwd on the remote host |
| **3** | Session runs `grok` **on the host** (remote cwd) | Disconnect persistence |
| **4** | Reconnect via `grok agent leader --no-exit-on-disconnect` | tmux UI |

## Hard rules (wave 1)

- Transport is system **OpenSSH** (`ssh` argv). Do not add a second SSH stack.
- Host aliases are concrete names only (`Host *` / `?` / `!` skipped).
- Alias is a process argument, never interpolated into a local shell.
- Tests use `BatchMode=yes` (keys only; no password prompt).
- First-time host keys: tell the user to run `ssh <alias>` once in a terminal (`StrictHostKeyChecking=yes`).
- Remote login check is **presence of `~/.grok/auth.json`**, never file contents.
- Remote install command is the official POSIX installer: `curl -fsSL https://x.ai/cli/install.sh | bash`.
- Remote login command is `grok login --device-auth` (TTY via `ssh -t`).
- Persistence is **not** tmux. Wave 4 uses Grok Build `agent leader --no-exit-on-disconnect`.
- Do not treat a remote path as a local filesystem path.

## Settings

- Section `runtime`, tab `ssh`, hash `#/settings/runtime/ssh`.
- Catalog id `runtime.sshHosts`, anchor `settings-anchor-sshHosts`.
- Host commands: `ssh_list_hosts`, `ssh_test_host`, `ssh_watch_start`, `ssh_watch_stop` in `src-tauri/src/ssh_remote.rs`.
- Watch switch: thumb and Watching / Available partition update immediately. `ssh_watch_start` / `ssh_watch_stop` run after. Failure reverts the switch and shows an error. Missing remote CLI or login is a warning, not a silent no-op.
- `-o ControlPath=` is ssh_config. Quote values with spaces (`Library/Application Support`). Sockets live under the app **cache** dir (`ssh-cm`), not data dir. Tests: `openssh_accepts_quoted_controlpath_with_spaces` and `SshHostsPanel.test.tsx`.
- Sidebar remote rail: one group per watching host, heading `远程 {alias}` / `Remote {alias}`. Session title is custom name (local overlay, double-click to edit) else first user sentence from remote `chat_history.jsonl` / `summary.json` else cwd basename. Never show the raw agent session id as the title. Full remote cwd is a hover Tip, not a wrapping subtitle. List newest 20, then **Load more**. Click runs `ssh_open_session`: fetch remote `chat_history.jsonl` (fallback `updates.jsonl`), import into a local App journal, then open that session so the transcript is visible. Wave 3 still does not resume grok on the host. Remote right-click menu parity is a later TODO.
