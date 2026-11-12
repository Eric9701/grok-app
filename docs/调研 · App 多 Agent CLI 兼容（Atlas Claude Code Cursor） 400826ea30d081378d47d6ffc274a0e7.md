# 调研 · App 多 Agent CLI 兼容（Atlas / Claude Code / Cursor）

## MULTI-CLI-A 短验收（大禹认 · 2026-10-21）

飞天挂 P1–P4：探测 / 开聊流式 / 取消 / Atlas 零回归。箱附件：`knowledge-handoff/research/feitian-ag1-multi-cli-runtime-acceptance.md`。与升级刀**分刀**。

## 大禹锁刀（2026-10-21）

- **切面 A**：Host 内 Runtime Adapter（不接 Multica 式 daemon）。
- **首期 provider**：Atlas（零回归）+ Claude Code + Cursor Agent。
- **形态**：纯本机 GUI。
- 架构：抽 `AtlasRuntime` → Claude/Cursor 适配器（ACP vs `stream-json`）；UI 只吃归一事件。
- 飞天挂短验收：探测 / 开聊流式 / 取消 / Atlas 零回归。

## 对齐记录（2026-10-21）

与 @飞天 结论一致：**切面 A = Host 内 Runtime Adapter**；Claude/Cursor 走 stream-json，勿扩 ACP 单通道。落地蓝本以飞天附件为准：`knowledge-handoff/research/feitian-atlas-app-multi-cli-compat.md`（房间已贴）。

来源：调研小组 · 2026-10-21

## 问题

当前 [grok-app `feat-atlas`](https://github.com/Eric9701/grok-app/tree/feat-atlas)（用户口中的 feature-atlas；仓库实际分支名是 **`feat-atlas`**）作为「Agent 调用 App」，深度定制对接 [grok-build `feature-atlas`](https://github.com/Eric9701/grok-build/tree/feature-atlas) 的 **Atlas CLI**（`atlas` / 同源 `grok`）。目标是**兼容 Claude Code、Cursor Agent 等**，参照 [multica](https://github.com/Eric9701/multica)。

## 现状：Atlas 单轨

### App（Host）

- 桌面白标 Atlas；探测顺序 `atlas` → 回退 `grok`；shared home `~/.atlas`（`GROK_HOME`）。
- **唯一主协议：ACP**。spawn：`{cli} agent --always-approve stdio`（见 `docs/llm-wiki/atlas.md`、`docs/SPIKE-ACP.md`）。
- Host 不重写 agent 脑；会话 / 权限反 RPC / 流式语义事件都挂在 ACP 上。
- 产品面大量 Atlas 专属：设备码登录、企业 CLI 镜像、Relay Agent、模型目录读 `~/.atlas` 的 `config.toml` / cache。

### CLI（Agent）

- Grok Build / Atlas：Rust TUI + `agent` ACP stdio；文档明示可嵌进编辑器 via ACP。

### 结论

App 今天是 **「单一 ACP 子进程 + Atlas 品牌/家目录」**，不是「多 Provider 适配层」。要加 Claude / Cursor，不能只改二进制名。

## Multica 怎么做（可参照）

架构：Web/桌面 → Go 后端 → **本机守护进程** → 按 Provider **拉起已安装 CLI**。

关键：`server/pkg/agent`

- 统一 `Backend` 接口：`Execute(prompt, ExecOptions) → Session{Messages, Result}`。
- `SupportedTypes` 白名单（claude / cursor / grok / codex / … 二十多种）。
- **每种 CLI 一个适配器文件**，协议族不同：
    - **Claude Code**：`claude -p --output-format stream-json --input-format stream-json`（自有 JSON 协议 + control_request）。
    - **Cursor Agent**：`cursor-agent` stream-json 族（另有 hook / 后台进程处理）。
    - **Grok / Atlas 同类**：`grok --no-auto-update agent --always-approve [--effort] stdio`（**ACP**，与 App 同路径）；Multica 已有完整 `grokBackend`。
- 另有 `BuiltinRuntime`：同一协议族可挂多个命令身份（如 omp→pi）。
- 守护进程负责探测 PATH、用量、重试、运行中插话（Supplement）等。

要点：**Host 统一任务模型；差异关进 Provider Backend**。不假设所有 CLI 都说 ACP。

## 协议对照（兼容难点）

| Provider | 典型拉起 | 会话/流式 | 与现 App 差距 |
| --- | --- | --- | --- |
| Atlas / Grok | ACP `agent … stdio` | JSON-RPC + session/update | **已实现** |
| Claude Code | stream-json | assistant/user/result 事件 | 无 ACP；权限/AskUser/usage 语义不同 |
| Cursor Agent | stream-json / 自有 CLI | 独立生命周期与 hook | 不能当 atlas 别名 |
| Codex 等 | app-server 等 | 又一族 | Multica 单独 backend |

**错误路径**：给 Claude 套 Atlas ACP 握手 → 必然失败。

## 对 App 的改造建议（分层）

### 推荐切面：引入 Provider Runtime 层（学 Multica）

1. **抽象 `AgentRuntime`**（对标 Multica `Backend`）
    - 能力：探测可执行文件、建会话/续会话、prompt、流式事件归一、取消、权限/人机问答钩子（有则实现）。
    - 事件归一成 App 已有 UI 模型（思考 / 正文 / tool / 终态），避免每家一套气泡。
2. **先做 3 个适配器**
    - `atlas`（现有 ACP Host，几乎搬迁）。
    - `claude`（stream-json；参照 Multica `claude.go`）。
    - `cursor`（参照 Multica `cursor.go`）。
3. **设置 / Composer**：Provider 下拉 + 每家二进制路径 / home / 模型发现策略；默认仍 Atlas。
4. **不要**把 Claude/Cursor 硬塞进「只改 cli 名」的探测链。

### 可选捷径（不推荐作主方案）

- 只支持「也说 ACP」的 CLI：覆盖面窄（Cursor/Claude 主路径不是 ACP）。
- 嵌终端仿真跑 TUI：维护成本高（SPIKE 已否）。

### 与 Multica 的边界

|  | grok-app（桌面 Host） | Multica |
| --- | --- | --- |
| 形态 | 本机 Tauri GUI，一对一会话体验 | 看板 + 守护进程派活 |
| 可借鉴 | Provider 适配器模式、事件归一、探测 | 任务/看板/用量可后置 |
| 慎搬 | 整套 Go daemon + PG 工作区 | 过重 |

## 建议里程碑

1. **M0 调研收口（本文）**：确认分支名、协议矩阵、Multica 对照。
2. **M1 抽象**：抽出现有 ACP 为 `AtlasRuntime`；UI 只吃归一事件。
3. **M2 Claude**：只读 cwd 冒烟 + 工具循环 + 取消；权限策略对齐现 YOLO/ask。
4. **M3 Cursor**：同上；处理其特有 hook/后台。
5. **M4 产品**：Provider 切换、模型列表、失败诊断文案；Atlas 专属页（Relay/企业镜像）仅 Atlas 可见。

## 源码锚点

- App Atlas 白标：`docs/llm-wiki/atlas.md`
- App ACP SPIKE：`docs/SPIKE-ACP.md`
- Multica 后端入口：`server/pkg/agent/agent.go`（`SupportedTypes` / `New`）
- Multica Grok ACP：`server/pkg/agent/grok.go`
- Multica Claude：`server/pkg/agent/claude.go`
- Multica Cursor：`server/pkg/agent/cursor.go`

## 待确认（给大禹）

1. 首期目标是否只要 **Claude + Cursor**，还是跟 Multica 拉长名单？
2. App 是否坚持 **纯本机 GUI**（无 daemon），还是接受轻量本机代理进程？
3. Atlas 企业登录 / Relay 是否对非 Atlas Provider 隐藏即可？