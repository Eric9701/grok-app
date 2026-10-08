# Atlas 白标（feat-atlas）

本分支把桌面壳显示为 **Atlas**，并优先对接企业内部 **Atlas CLI**。对照源是内部 grok-build（命令 `atlas`，家目录 `~/.atlas`）。**不要**把这些改动合进对外 `main` 的 README / GitHub Release 话术（除非另开发行任务）。

## 不变

| 项 | 说明 |
|----|------|
| ACP | 仍 spawn `{cli} agent --always-approve stdio`，Host 不重写 agent |
| 环境变量名 | CLI 读的仍是 `GROK_HOME` / `GROK_CONFIG` / 其它 `GROK_*` |
| 模型 id | `grok-4.6` / `grok-4.5` 等上游名不改 |
| 皮肤协议 | 仍用 `grok://` / `.grokskin`（第二期再加 `atlas://`） |
| 独立模式 | 仍写 App `agent-home`，**禁止改写** shared CLI home |

## 探测顺序

`cli_probe` / PATH 补强 / sidecar / WSL：

1. 二进制名：**`atlas` 优先**，再回退 `grok`（含 `.exe` / `.cmd` / `.bat`）。
2. 官方布局：`~/.atlas/bin`、`~/.atlas/downloads/atlas-*`（或发布名仍为 `grok-*` 的包）。
3. 回退：`~/.grok/bin` 与历史 `grok` 路径。
4. sidecar `agent`：与探测到的 CLI **同目录**，再 `~/.atlas/bin/agent`，再 `~/.grok/bin/agent`。
5. WSL 默认命令：`atlas`，回退 `~/.atlas/bin/atlas`。
6. 版本 banner：`atlas 0.2.134 (…)` 与 `grok 0.2.x` 都能解析。推荐线 **0.2.134**（最低线仍 0.2.112），避免 Atlas 0.2.x 被 1.0 芯片永远催升级。

GUI 进程 PATH 很瘦：`enriched_path_env` 会补上 `~/.atlas/bin`。

## 家目录

| 模式 | `GROK_HOME` 默认路径 |
|------|----------------------|
| shared（默认） | `~/.atlas`（与终端 `atlas` 同一套 config / 会话 / 插件） |
| independent | App `agent-home`（`$GROK_APP_HOME/agent-home` 或 `~/.atlas-app/agent-home`） |

自定义中转仍写 App `agent-home/config.toml`，spawn 时 `GROK_HOME` 指过去（#557 行为不变）。

## 远程更新（已关闭）

本白标**不**向 GitHub / 官方 updater 探活，也**不**在启动后跑 `atlas update --check`：

- 应用：无后台检查、设置 → 关于 无「检查更新」、侧栏无更新徽章
- CLI：设置里的更新行只说明已关闭；不代跑 `atlas update` / 切通道 / 钉版本
- 首次安装 CLI 仍走企业 `atlas/cli` 基址（`cli_install`），与「远程更新」探测分开

开关：`src/lib/remoteUpdates.ts` 与 `src-tauri/src/remote_updates.rs` 的 `REMOTE_UPDATES_ENABLED`（当前 `false`）。

## Relay Agent

设置 → 运行时 → 连接 里的 **Atlas Relay** 让本应用作为执行端出站连 `atlas-relay-demo` 的 `/ws?agent_id=`。中继把 ACP 帧转进来，Host 原样写给本机 `atlas agent --always-approve stdio`，再把 stdout 的一行回成一条 WebSocket 文本帧。工具在这台机器上执行。桌面会话仍走原来的本地 CLI / ACP 服务器 / SSH，不进这条桥。

登记：`settingsCatalog` 的 `runtime.atlasRelayAgent`。字段：`atlasRelayAgentUrl`、`atlasRelayAgentId`、`atlasRelayAgentEnabled`。健康检查间隔是 `atlasRelayAgentHealthSecs`（`runtime.atlasRelayAgentHealth`）：在线后按该秒数发 WebSocket Ping，下一次到期前没有 Pong 就断开并重连。默认 15 秒，范围 5–300。改间隔不重启桥。

侧栏「Atlas云端」只读展示这条桥上的 `session/prompt` 及完成、失败、取消状态。只有 `online` 显示已连接。桌面聊天不受影响。

## 安装 / 更新通道

- **已装 CLI**：`atlas update` / `atlas update --check --json`。
- **未装 / App 代下**：只放行企业基址（允许内网 `http`），**不把 x.ai / GCS 当自动安装源**。
- 解析顺序：环境变量 `ATLAS_CLI_MIRROR` → 设置 `atlasCliMirror` → Host 内置默认（企业 `…/atlas/cli`）。
- 发布文件名仍可能是 `grok-{ver}-{os}-{arch}`；安装落点是 `~/.atlas/bin/atlas`（Windows：`atlas.exe`）+ `agent`。
- 向导「复制安装命令」：Windows `install.ps1` / Unix `install.sh`，基址同上。

新设置必须登记 `settingsCatalog`：`runtime.atlasCliMirror`（Runtime → CLI）。

## 模型列表

Composer / 设置「可用模型」走 Host `models_list_available`（`src-tauri/src/models_catalog.rs`），**不是**直接跑 `atlas models`。

查找顺序：

1. `models_cache.json`：live `GROK_HOME`（shared = `~/.atlas`）→ 再扫 `~/.atlas` → 历史 `~/.grok`
2. 同一套家目录里的 `config.toml` **`[model.<catalog-id>]`**（企业托管目录；`atlas models refresh` / `/refresh-model` 会同步到这里）
3. 都空才硬插 `grok-4.6`

Atlas 企业环境常常**没有** `models_cache.json`（或托管 ENC 校验失败后整文件作废），目录只在 `config.toml`。旧逻辑故意不合并 `[model.*]`（对外 Grok App 把它们当自定义渠道），白标下会导致 Composer 看不到 `arch-kimi-for-coding` 等分配模型。

默认：`[models].default`（例如 `arch-kimi-for-coding`）。展示名用明文 `name`，跳过 `ENC(...)` 与 `hidden = true`。密钥不下发前端。

立刻刷新企业目录：终端 `atlas models refresh`（或会话 `/refresh-model`），然后重开 Composer / 再进设置。

## 账号主路

向导第一步仍是「必须有可运行 CLI」。第二步主路是 **Atlas 设备码**（`atlas login --device-auth`）和自定义中转。grok.com OAuth / console.x.ai 留在设置里的高级项。

## 与官方 Grok App 并排

本机可以同时装着官方 Grok App 与本白标：

| | Atlas（本分支） | 官方 Grok App |
|--|----------------|---------------|
| `productName` | Atlas | Grok |
| identifier | `com.atlasapp.desktop`（dev：`.dev`） | `com.grokapp.desktop` |
| App 数据 | `atlasapp` / `atlas-app`（`GROK_APP_HOME` 仍可覆盖） | `grokapp` / `grok-app` |
| CLI home（shared） | `~/.atlas` | `~/.grok` |

single-instance、任务栏、钥匙串与 App 数据互相隔离。

## 测试注意

公开测试常量**不要**写内网 IP。allowlist 用 `is_allowed_download_url_against` + 夹具 host（如 `cli.example.test`）。
