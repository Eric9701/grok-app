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

例外：设置里的 **从 CC Switch 导入** 会把自定义 `[model.<id>]` 合并进共享 `~/.atlas/config.toml`，供终端 `atlas` 按段 id 选用。不改 `[models].default`，不覆盖 `api_key = "ENC(...)"` 或 `hidden = true` 的企业段，也不导入 Grok Official。其它写入仍禁止改 shared CLI home。

## 远程更新（已关闭）

本白标**不**向 GitHub / 官方 updater 探活，也**不**在启动后跑 `atlas update --check`：

- 应用：无后台检查、设置 → 关于 无「检查更新」、侧栏无更新徽章
- CLI：设置里的更新行只说明已关闭；不代跑 `atlas update` / 切通道 / 钉版本
- 首次安装 CLI 仍走企业 `atlas/cli` 基址（`cli_install`），与「远程更新」探测分开

开关：`src/lib/remoteUpdates.ts` 与 `src-tauri/src/remote_updates.rs` 的 `REMOTE_UPDATES_ENABLED`（当前 `false`）。

## Relay Agent

设置 → Atlas 里的 **Atlas Relay** 让本应用作为执行端出站连 `atlas-relay-demo` 的 `/ws?agent_id=`。填了 Token 时再带 `&token=`。中继把 ACP 帧转进来，Host 写给本机 `atlas agent --always-approve stdio`（这条连接若选了模型，会改写 `session/new` 与 `session/set_model` 的 Catalog ID），再把 stdout 的一行回成一条 WebSocket 文本帧。本机注入的 `session/set_model` 回复不转回中继。工具在这台机器上执行。桌面会话仍走原来的本地 CLI / ACP 服务器 / SSH，不进这些桥。

最多 4 条 Agent。每条有自己的地址、Token、Agent ID 和启用状态，存在 `atlasRelayAgents`。Agent ID 去首尾空白、去掉换行、最长 128 之后不能重复，地址不同也不行。空 ID 不入库。第 5 条和重复 ID 会被拒绝。Token 可空；空则这条连接不带该参数。把带 `agent_id` 和 `token` 的整段地址贴进某一行，只拆进这一行；拆出来的 ID 若与其他行重复，只提示，不改其他行。状态和日志只保留打码后的地址。

改某一条的地址、Token 或 ID，只重连这一条。每条连接单独保存 Catalog ID（`modelId`）。下拉留空则不改云端帧。选了模型后，这条连接上的 `session/new` 和 `session/set_model` 都改写成该 id；已经见过的会话会立刻切过去，不断开 WebSocket，也不重启 CLI。改回留空只停止改写。健康检查间隔仍是整张卡片的一份：`atlasRelayAgentHealthSecs`（`atlas.relayAgentHealth`）。每条在线连接按该秒数发 WebSocket Ping，下一次到期前没有 Pong 就只断开这一条并重连。默认 15 秒，范围 5–300。只改间隔不重启任何桥。

旧的单个 `atlasRelayAgentUrl` / `atlasRelayAgentId` / `atlasRelayAgentToken` / `atlasRelayAgentEnabled` 只在列表为空时迁成第一项，然后清空，避免删光列表后又被旧字段复活。

登记：`settingsCatalog` 的 `atlas.relayAgent`。任务带 `agentId`。看板卡片 id 是 `relay:${agentId}:${taskId}`。侧栏「Atlas云端」在任意一条为 `online` 时显示已连接。

同页的 **Atlas云端站点**（`atlasCloudSiteUrl`，`atlas.cloudSite`）是侧栏「Atlas云端」打开的网页。只接受 http 或 https；留空则显示去设置的入口，不挂空白网页。切换菜单时页面保持，地址栏不显示，刷新和用系统应用打开仍在。点击看板里的中继卡片仍打开 `#/atlas-cloud`。

智能体看板把这条桥上的 `session/prompt` 当作带来源标记的卡片放进现有列：进行中、已完成、失败（需要你）。取消的不进看板。点卡片打开 `#/atlas-cloud`（上面的站点），不打开桌面会话，也不写入会话列表。

## 安装包文件名

Release 门禁认 `src-tauri/tauri.conf.json` 的 `productName`，也就是 **Atlas**。四个平台都要有：

- `Atlas_<版本>_aarch64.dmg`、`Atlas_<版本>_x64.dmg`
- `Atlas_<版本>_x64-setup.exe`、`Atlas_<版本>_x64-portable.zip`
- `Atlas_<版本>_amd64.AppImage`、`.deb`、以及 `Atlas_<版本>-1.x86_64.rpm`

同一 tag 上残留的 `Grok_*` 安装包不算数。绿色版 zip 和官网别名（`Atlas_mac_x64.dmg` 等）用同一前缀。

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
