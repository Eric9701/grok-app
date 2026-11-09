# Atlas Fleet

AtlasBuddy 作为 leader：本机排队、查看任务；远程机器执行已有命令 `atlas agent --always-approve headless --grok-ws-url …`。本机任务由 fleet 自己拉起 `atlas agent --always-approve stdio`，不拨 WebSocket。

## 监听

- 默认绑定 `127.0.0.1:24720`，路径 `/ws`。
- 查询参数是身份：`agent_id` 与 `token`。回环和非回环都要 token。非回环没有 token 不能开监听。
- 一帧一行 ACP。空白和文本 `ping` 丢掉。健康检查是 WebSocket Ping（15 秒），约 45 秒没有 Pong、文本或 Ping 就关掉并标离线。
- 同一 `agent_id` 的新连接替换旧连接，不续半截 ACP。最多 32 条连接。`local` 保留给本机，不能当远程机器名。
- 状态里只出现 token 末四位。复制用的 worker 命令只在 `fleet_listen_apply` 成功的那一次响应里给出。
- 「已保存」和「正在监听」是两件事。绑定失败是错误，名册保持空。

设置字段：`fleetListenEnabled`、`fleetBind`、`fleetToken`。保存走 `fleet_listen_apply`，不进 `settings_set` 的分支。

## 任务文件

只写应用数据目录（`paths::app_data_root()`）下的 `fleet/tasks/`：

- `{id}.json`：机器、远端绝对 `cwd`、提示词、状态、失败原因、时间、RPC id、`sessionId`、`retryOf`、不可达标记。临时文件换名。列表摘要约 240 字。
- `{id}.steps.jsonl`：只追加折好的 `thought` / `message` / `tool`，最多 200 步、单步 4000 字。不存原始帧。读的时候丢掉坏掉的最后一行。

状态：`queued` → `claimed` → `running` → `completed` / `failed` / `cancelled`。每台机器同时只有一条仍在进行且可达的任务。leader 不在本机检查远端 `cwd` 是否存在，但相对路径会被拒绝。

断线：`claimed` / `running` 标不可达，15 分钟后失败，原因 `runtime_offline`。宽限内重连不接半截会话；`runtime_offline` 只自动重试一次，新任务带 `retryOf`。已结束任务保留最近 200 条。排队和未结束的不删。重启只读盘：排队保持排队，进行中的标不可达。

找不到本机 CLI 时任务失败，原因 `runtime_missing_executable`。`session/new` 准备失败的原因是 `environment_prepare_failed`。

## 和现有通道的分界

- 桌面会话仍走现有 CLI / ACP 服务器地址。fleet 不调用 `session_connect`。
- Atlas Relay 仍是本应用向外连接云端。fleet 不引用 `atlas_relay_agent` 的 hub。
- `agent serve` 仍是 `127.0.0.1:2419`。
- SSH 与看板的 `relay:` 卡片不读 fleet 任务。
- 不写 `~/.atlas`，不把任务写进 `settings.json`。worker 自己的 CLI 会话留在那台机器的 `~/.atlas/sessions/`。

界面是 `#/fleet`。点名册里的机器就填进派发表单；还没改过的默认 `local` 会换成在线的远程机器。工作目录是目标机器上的绝对路径，相对路径会直接说明原因。失败行显示掉线、准备失败、找不到本机 Atlas 或执行失败。排队行标明在等这台机器；带 `retryOf` 的行标明重试的是哪一条。设置卡在 Atlas 分区，登记 id 为 `fleet.listen`。
