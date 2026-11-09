# 调研：LLM Wiki 与 AtlasBuddy

**日期：** 2026-11-08  
**上游：** [Eric9701/llm_wiki](https://github.com/Eric9701/llm_wiki) @ `e08cfce65aad8b76b13546a081c1a8d4422b70c3`（`release: v0.6.13`，作者 nash_su，2026-10-21）  
**问题：** 这个仓库实际提供什么，AtlasBuddy（`feat-atlas` 白标桌面壳）应该怎样接，才不会把另一套桌面应用嵌进来。

阅读范围：GitHub API 上该提交的完整文件树（517 个路径，`truncated: false`），以及 README、`llm-wiki.md`、`LICENSE`、schema / 模板、HTTP、MCP、Agent、Lint、剪藏相关源码。同一提交的 zip 已解压到临时目录（约 23.9 MB，目录名带 `e08cfce`），没有放进 grok-app。浅克隆没有完成。GitHub compare `nashsu:main...Eric9701:main` 的 `status` 为 `identical`，`ahead_by` / `behind_by` 均为 0。Eric9701 这个 fork 创建于 2026-11-08，没有自己的提交。下文路径都指这个提交里的文件。

## 判词

| 问题 | 结论 |
|------|------|
| 它是什么？ | **另一套 Tauri 桌面应用。** 用户自选一个项目目录，应用用用户配置的模型把原文增量写成互相链接的 Markdown Wiki。 |
| 它是 Karpathy 那份可粘贴给代理的模式文档吗？ | **仓库里夹着那份模式，产品不是它。** 根目录 `llm-wiki.md` 写明自己是抽象想法；README 把桌面应用称为该想法的实现。 |
| 有没有名为 compile 的编译器或 CLI？ | **没有。** 唯一的应用二进制是桌面程序 `llm-wiki`。最接近「编译」的是设置里的 `rebuild_wiki_index`，只重写 `wiki/index.md`。 |
| 代理怎么用它？ | **桌面进程开着时，打本机 HTTP。** 仓库内的 stdio MCP 只是这个 API 的客户端。给 Claude / Codex 的 skill 在另一个仓库，内容是 HTTP 说明，不是本仓库里的 `SKILL.md`。 |
| AtlasBuddy 该不该把它编译进壳里？ | **不要。** 根许可证是 GPL-3.0，AtlasBuddy 根 `LICENSE` 是 MIT。它还有自己的聊天 Agent、模型配置、图谱和摄入队列。 |
| 现在有没有知识库界面可挂？ | **没有 Wiki / 笔记工作台。** 侧栏资源是文件、浏览器、终端、技能、审查、计划。设置里的记忆浏览器是代理记忆，不是这套 Wiki。 |

**推荐形态：** LLM Wiki 继续作为用户单独安装、单独打开的桌面应用。AtlasBuddy 这一期不新增界面、不调用 `mcp_add`、不把插件装进 CLI 家目录。若要让 Atlas 代理查询，由用户把上游 HTTP skill 放到**当前项目** `.grok/skills/`，并自己让代理进程看得到 `LLM_WIKI_API_TOKEN`。图谱、摄入、审核留在 LLM Wiki 窗口。

---

## 1. 仓库事实：它是什么

README「What is this?」：跨平台桌面应用，把文档收成会持续更新的知识库；知识编译一次后保持更新，而不是每次提问都从原文重推。同一节写明方法论来自 Karpathy 的 [llm-wiki gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)，实现者是 nash_su。仓库内落地的是根目录 `llm-wiki.md`（一份可复制给代理的抽象说明），不是把 gist 的目录约定原样当成代码。

`llm-wiki.md` 自己写：它描述想法，不描述某一种实现；原文不可修改，Wiki 由模型维护，schema 告诉模型目录和流程；操作是 Ingest、Query、Lint；`index.md` 是目录，`log.md` 是追加日志。README「What We Kept」确认桌面应用保留了这三层和这三个操作，并增加了 `purpose.md`、两步摄入、图谱、向量检索、审核队列、深度研究、剪藏和本机 API。

它不是：

- 一份只靠代理读写 Markdown 的空目录规范。规范在 `llm-wiki.md` 和每个项目的 `schema.md`；真正跑起来的是桌面应用。
- AtlasBuddy 仓库里的 `docs/llm-wiki/`。那是本仓库给改产品的代理看的规则（`docs/llm-wiki/README.md`、根 `AGENTS.md` 第 1 条），和 Eric9701/llm_wiki 没有代码关系。
- 可被 `atlas agent` 直接当会话后端的服务。它的聊天 Agent 在它自己的进程里。

技术形态（`package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`，版本都是 `0.6.13`）：

| 部分 | 路径 | 角色 |
|------|------|------|
| 桌面壳 | `src-tauri/`，产品名 LLM Wiki，标识 `com.llmwiki.app` | Tauri 2 窗口、文件、API、剪藏、向量 |
| 界面 | `src/` | React 19。README 写的侧栏是 Wiki、Sources、Search、Graph、Lint、Review、Deep Research、Settings |
| MCP | `mcp-server/` | Node 20+，stdio，bin 名 `llm-wiki-mcp` |
| 剪藏 | `extension/` | Chrome 扩展，快捷键把网页交给桌面 |
| 模式原文 | `llm-wiki.md` | 抽象说明，应用不执行它 |
| 许可证 | 根 `LICENSE` | GNU GPL-3.0，版权 Yong Su，2024–2026。README「License」同此 |

`mcp-server/package.json` 的 `"license"` 写成 `MIT`，该目录没有第二份 LICENSE。根 LICENSE 与这个字段冲突。在上游写清之前，不要把 `mcp-server/` 源码抄进 MIT 的 AtlasBuddy。

依赖（事实，不是建议）：前端含 Milkdown、sigma.js、Lance 不在前端；Rust 侧有 `lancedb`、`pdfium-render`、`anydoc`、`docx-rs`、`calamine`、`epub`、`mobi`、`tiny_http`、`notify`。构建要求 Node.js 20+、Rust 1.88+、protoc（README「Build from Source」）。预编译包在 README 里指向 `nashsu/llm_wiki` 的 Releases（macOS dmg、Windows msi、Linux deb/AppImage）。这次没有下载安装包，也没有核对安装包是否等于 `e08cfce`。

## 2. 仓库事实：目录与数据

项目在用户选的文件夹里，不在 Atlas 家目录。README「Project Structure」：

```text
my-wiki/
├── purpose.md                 # 目标、问题、范围
├── schema.md                  # 页类型和目录规则
├── raw/sources/               # 原文。产品规则是模型只读
├── raw/assets/
├── wiki/index.md              # 内容目录
├── wiki/log.md                # 操作记录
├── wiki/overview.md
├── wiki/entities|concepts|sources|queries|synthesis|comparisons/
├── .obsidian/                 # 应用生成，可当 Obsidian 库
└── .llm-wiki/                 # 应用状态
```

`src/types/wiki.ts`：项目 id 是稳定 UUID，存在该项目的 `.llm-wiki/project.json`，文件夹改名后 id 还在。

`src/lib/templates.ts` 的通用页类型表（模板写进 `schema.md`，不是 Rust 枚举）：

| type | 目录 |
|------|------|
| entity | `wiki/entities/` |
| concept | `wiki/concepts/` |
| source | `wiki/sources/` |
| query | `wiki/queries/` |
| comparison | `wiki/comparisons/` |
| synthesis | `wiki/synthesis/` |
| overview | `wiki/` |

研究、读书、个人、商务模板会再加 `wiki/thesis/`、`wiki/characters/` 等。`src/lib/wiki-schema.ts` 只解析 `schema.md` 里标题为 Page Types 的表格，并要求目录是 `wiki` 或 `wiki/` 之下，拒绝 `..`。类型和目录不一致时，应用可以把页面改放到表中的目录。

索引和日志的格式也在模板字符串里，不是单独的 JSON schema：

- `wiki/index.md`：按类型分组，条目形如 `- [[page-slug]] — 一行说明`。
- `wiki/log.md`：倒序，`## YYYY-MM-DD` 下用列表记动作。这比 `llm-wiki.md` 举例的 `## [日期] ingest | 标题` 更松。

应用自己的状态（源码中的路径，都在项目目录内，除非另注）：

| 数据 | 位置 | 来源 |
|------|------|------|
| 向量 | `{项目}/.llm-wiki/lancedb` | `src-tauri/src/commands/vectorstore.rs` `db_path` |
| 审核队列 | `{项目}/.llm-wiki/review.json` | `src-tauri/src/api_server.rs` |
| 聊天 | `{项目}/.llm-wiki/chats/{id}.json` | README「Multi-Conversation」 |
| 项目技能 | `{项目}/.llm-wiki/skills/`，以及用户主目录 `.claude/skills`、`.codex/skills`、`.agents/skills` | `src-tauri/src/agent/skills.rs` `skill_roots` |
| Agent 生成物 | `{项目}/agent-workspace/` | `src-tauri/src/agent/workspace.rs` |
| 桌面全局设置 | Tauri `app_data_dir()/app-state.json`（标识 `com.llmwiki.app`） | `api_server.rs` `load_app_state`、`server_bind.rs` |

这次没有在一台已安装的机器上打开该应用数据目录，所以上表是源码路径，不是本机实测目录。

## 3. 仓库事实：索引、页面、日志、四条操作

没有叫 `compile` 的命令、脚本或 MCP 工具。文件树里也没有 `compile` 源文件。README 里的 “compiled once” 是在描述 Wiki 页会留下来。设置维护页调用 Tauri 命令 `rebuild_wiki_index`（`src/components/settings/sections/maintenance-section.tsx`），只重建索引。

| 操作 | 在这个仓库里实际是什么 | 代理接口有没有同名工具 |
|------|------------------------|------------------------|
| Ingest | 桌面里的两步模型调用：先分析，再写 Wiki 页、`index.md`、`log.md`、`overview.md` 和审核项。SHA256 缓存、串行队列、`raw/sources/` 监视。README「Two-Step Chain-of-Thought Ingest」；实现集中在 `src/lib/ingest.ts` 及同目录 `ingest-*.ts` | 没有 `ingest` 工具。`llm_wiki_rescan_sources` 按 Source Watch 重扫，可能接着走摄入或删除 |
| Query | 桌面聊天的检索：分词、可选 LanceDB 向量、图谱扩展、上下文预算，再把 `purpose.md` 和 `index.md` 放进提示。README「Optimized Query Retrieval Pipeline」 | `llm_wiki_search` 是检索；`llm_wiki_chat` 是它自己的 Agent 回答 |
| Lint | `src/lib/lint.ts`：`runStructuralLint` 扫 `wiki/` 的链接和孤立页（跳过 `index.md`、`log.md`）；`runSemanticLint` 再调模型。界面在 `src/components/lint/` | HTTP / MCP 都没有 lint 路由 |
| 索引重建 | `rebuild_wiki_index` | 没有 |

页面是带 YAML frontmatter 的 Markdown。README 写明每页有 `sources: []`，交叉引用用 `[[wikilink]]`。查询答案可以存进 `wiki/queries/`。原文规则是不可变；剪藏例外，见第 6 节。

## 4. 仓库事实：怎么运行

| 方式 | 是否存在 | 依据 |
|------|----------|------|
| 桌面应用 | 是，主路径 | `src-tauri/src/main.rs` 对应的 bin `llm-wiki`；`npm run tauri dev` / `tauri build` |
| 本机 HTTP | 是，随桌面进程 | `API_SERVER_PORT = 19828`（`src/lib/api-server-constants.ts`）；`tiny_http`（`api_server.rs`） |
| stdio MCP | 是，必须另有桌面 API | `mcp-server/README.md`：不扫文件夹，不复制检索逻辑，每个工具都打 `http://127.0.0.1:19828/api/v1` |
| Skill | 不在本仓库 | README 指向 `https://github.com/nashsu/llm_wiki_skill`，命令 `npx skills add … --skill llm-wiki` |
| 纯文件 | Wiki 目录可被 Obsidian 打开 | 没有桌面进程时，没有 HTTP、没有摄入队列、没有向量库服务 |
| 独立 wiki CLI | 没有 | Cargo 只有桌面 bin。`claude_cli.rs` / `codex_cli.rs` 是本应用把 `claude` 或 `codex exec --json` 当作补全引擎拉起来，注释写明不把那些 CLI 的工具和 MCP 交给网页 |

Claude / Codex 传输不能当成 Atlas 的接缝：它们服务 LLM Wiki 自己的模型配置。

## 5. 仓库事实：和代理的接口

### HTTP

`api_server.rs` 的路由（`handle_request`）：

| 方法 | 路径 | 作用 |
|------|------|------|
| GET | `/api/v1/health` 或 `/health` | 状态。API 被设置关掉时仍然可访问 |
| GET | `/api/v1/projects` | 已知项目 |
| GET | `…/files`、`…/files/content` | 列文件、读文本 |
| GET | `…/reviews` | 审核项 |
| POST | `…/reviews/resolve` | 批量解决 |
| PATCH / DELETE | `…/reviews/{id}` | 改或删一条审核。MCP 工具列表没有这两项 |
| POST | `…/search` | 关键词 + 向量 + 一跳图谱 |
| GET | `…/graph` | 图谱 |
| POST | `…/sources/rescan` | 重扫 |
| POST | `…/pages/embed` | 给已有 `wiki/*.md` 做向量 |
| POST | `…/pages/write` | 写一页 |
| POST | `…/chat` | Rust Agent。可 SSE |
| POST | `…/chat/{sessionId}/cancel` | 取消 |

`id` 可以是 UUID、项目路径或 `current`（MCP 的 `project-binding.ts` 与 README 一致）。

读文件白名单 `is_public_project_rel`：只允许 `purpose.md`、`schema.md`、`wiki/`、`raw/sources/`。路径中任一段以 `.` 开头即拒绝，因此 `.llm-wiki/` 不能经这个接口读出。文本扩展名限于 md/mdx/txt/csv/json/yaml/yml/xml/html/htm/log，单文件最多 2MB。`safe_join` 拒绝绝对路径、`..`，并对已存在路径做 canonicalize，防止跳出项目。

写页面：`wiki.write_page`（`agent/tools.rs`）只接受项目内 `wiki/` 下的 Markdown，默认拒绝覆盖，拒绝覆盖符号链接。MCP 侧再限制路径：必须正斜杠、以 `wiki/` 开头、以 `.md` 结尾、不能有空段、`.`、`..` 或以 `.` 开头的段；正文最多 2MB（`mcp-server/src/index.ts`）。

### MCP 工具

`mcp-server/src/index.ts`，stdio，服务名 `llm-wiki`。桌面设置里 MCP 关闭时，除 `llm_wiki_status` 外都会失败。

| 工具 | 主要参数 | 结果 |
|------|----------|------|
| `llm_wiki_status` | 无 | health + 项目列表 + 本进程是否已绑定项目 |
| `llm_wiki_projects` | 无 | 项目列表 |
| `llm_wiki_set_project` | `project_id` | 绑定本 MCP 进程。之后别的项目 id 会被拒绝 |
| `llm_wiki_files` | `project_id`，`root=wiki\|sources\|all`，`recursive`，`max_files` | 文件树 |
| `llm_wiki_read_file` | `path` | 文本，MCP 再截到约 120000 字节 |
| `llm_wiki_reviews` | `status`，`type`，`limit` | 默认未解决 |
| `llm_wiki_search` | `query`，`top_k`，`include_content` | 检索结果 |
| `llm_wiki_chat` | `message`，`session_id`，`mode=fast\|standard\|deep\|local_first`，`wiki`，`web`，`anytxt`，`skills` | 它自己的 Agent 回复、引用、工具事件。有 `session_id` 时会把会话存进 LLM Wiki 项目 |
| `llm_wiki_graph` | `q`，`node_type`，`limit`，`offset`，`edge_scope` | 图谱 |
| `llm_wiki_rescan_sources` | `project_id` | 触发重扫 |
| `llm_wiki_embed_page` | `path`，`force` | 写向量 |
| `llm_wiki_write_page` | `path`，`content`，`allow_overwrite` | 写 `wiki/*.md`，成功要桌面端字节级核对 |

MCP 进程不执行 shell，也没有 lint / ingest / rebuild 工具。`llm_wiki_chat` 会进入桌面 Agent，那个 Agent 另有 shell，见下一节。

环境变量：`LLM_WIKI_API_TOKEN`、可选 `LLM_WIKI_API_BASE_URL`。README 要求令牌走环境变量，不要放进命令行参数。

### Skill（仓库外，README 点名的那个）

本仓库没有 `SKILL.md`。2026-11-08 读取的是 [nashsu/llm_wiki_skill](https://github.com/nashsu/llm_wiki_skill) 默认分支，未钉提交。根上只有 `README.md`、`SKILL.md`、`api-reference.md`、`examples.md`。README 写明：只有文档，没有客户端库，用 curl / fetch 打 HTTP。

`SKILL.md` 开头：只在用户点名 LLM Wiki、my wiki、知识库，或明确说重扫 / 图谱时触发；不要用在泛泛的「搜笔记 / Obsidian / Notion」。它列出的写操作只有 `POST …/sources/rescan`。正文写「Stay read-only by default」以及「不要发明写接口，v1 没有」。这和 `e08cfce` 的桌面 API 不一致：桌面已经有 `pages/write` 和 `chat`。skill 是一份更窄的 HTTP 说明，没有跟上写页面和聊天接口，也没有提到 MCP。

`npx skills add` 会落到哪个目录，这次没有执行，不能当成 Atlas 已经能看见。

## 6. 仓库事实：安全边界

默认绑定 `127.0.0.1:19828`（`server_bind.rs` 的 `DEFAULT_BIND_HOST`）。设置 `apiConfig.allowLanAccess` 为真时改为 `0.0.0.0`；环境变量 `LLM_WIKI_BIND_HOST` 优先。README「只本机」指的是默认，不是代码里唯一的绑定。

鉴权（`api_server.rs`）：

- 令牌来自环境变量 `LLM_WIKI_API_TOKEN`，否则来自 `app-state.json`。请求可用查询参数 `token`、头 `x-llm-wiki-token`，或 `Authorization: Bearer`。
- `/health` 不鉴权，并会返回 `mcpEnabled`、`allowUnauthenticated`、`allowLanAccess`。
- 未写配置时，API 总开关默认开，MCP 开关默认关，免鉴权默认关。没有令牌时，需要鉴权的请求是 401。
- `chat`、`pages/embed`、`pages/write` 即使打开了免鉴权也仍然要令牌（`is_token_required_request`）。搜索、读文件、图谱、重扫、审核在免鉴权打开时可以无令牌。
- 全局限流约每秒 120 个请求，同时约 64 个请求、8 路聊天流。

剪藏是另一个 HTTP 服务，端口 **19827**（`clip_server.rs` 常量 `PORT`，这次对过源码）。它把网页 Markdown 写到当前项目的 `raw/sources/`，再交给界面自动摄入。绑定主机和 API 共用 `configured_bind_host`。AtlasBuddy 不要连这个端口。

执行代码：

- 摄入和语义 Lint 调的是用户配置的模型 API，不是在 Wiki 目录里跑脚本。
- 桌面 Agent 的工具表包括 `wiki.search`、`wiki.read_page`、`source.search`、`web.search`、`graph.search`、`anytxt.search`、`wiki.write_page`、`skills.load`、`workspace.write_file`、`shell.exec`（`agent/tools.rs` `builtin_tool_specs`）。
- `shell.exec` 只有本回合加载了 skill 才可用（`agent/runtime.rs`）。工作区范围内的命令可以不经过批准列表；其余命令必须与 `approved_shell_commands` 里某一条在去掉首尾空白后完全一致。`tools.rs` 写明：环境变量被收窄，**这不是操作系统沙箱**，被批准的命令仍能访问用户文件系统。工作目录是 `agent-workspace/`。Windows 用 `ComSpec /C`，其它系统用 `/bin/sh -c`。
- `POST …/chat` 把请求体直接反序列化成 `AgentChatRequest`（`prepare_chat`），其中含 `skills` 和 `approved_shell_commands`，这段代码没有把后者清空。注释要求批准列表来自用户确认，而不是模型输出。MCP 的 `llm_wiki_chat` 参数里没有批准列表，因此这条 MCP 工具本身不会带上批准命令；能直接 POST JSON 的调用方可以带。skill 文档没有教代理去调 `/chat`。

不写的地方：公开读接口不暴露 `.llm-wiki/`；写页面不能写到 `wiki/` 以外；MCP 不能改审核项（HTTP 可以）。原文目录在产品说明里是不可变的，但剪藏和 Source Watch 会向 `raw/sources/` 加文件。

## 7. AtlasBuddy 事实：现有接缝

以下是本仓库 `feat-atlas` 里已经有的行为，不是 LLM Wiki 的能力。

| 接缝 | 现状 | 和这次集成的关系 |
|------|------|------------------|
| 插件 | 设置 → 扩展 → 插件。推荐项不自动安装，确认后 `plugin install --trust`。规则在 `docs/llm-wiki/plugins-marketplace.md` | LLM Wiki 仓库没有 `.grok-plugin` / `.mcp.json`。不能把它当成现成插件源 |
| 插件 MCP 扫描 | `src-tauri/src/plugin_mcp.rs` `discover_plugin_mcp_servers` 只扫 `~/.grok/installed-plugins` 和 `~/.grok/plugins` | 共享家目录已经是 `~/.atlas`（`src-tauri/src/paths.rs` `shared_cli_home`）。就算以后做成插件并装进 Atlas 家目录，这条扫描也看不到 |
| 手动添加 MCP | `ExtensionsPanelMcpModals.tsx` 有名称、命令、参数、环境变量。`mcp_add` → `add_mcp_stdio` | shared 模式写入 `mcp_agent_config_path`，即 `~/.atlas/config.toml`。这不在 `docs/llm-wiki/atlas.md` 列出的家目录例外（CC Switch 导入、追加模型、代装 CLI 的 endpoints）里 |
| 会话注入 | `docs/llm-wiki/slash-composer.md`：打开会话时把启用的 MCP 放进 ACP `session/new` / `session/load` | 服务器先进 CLI 配置，才会被注入。独立模式写的是 App `agent-home`，不是 `~/.atlas` |
| Skill | `skills_list` 合并 CLI inspect 与 `{项目}/.grok/skills/*/SKILL.md`。代理仍按磁盘发现技能。可另开「探测 Claude/Cursor 技能」，共享模式只改 App 覆盖层 | 项目内 `.grok/skills` 不需要写 `~/.atlas`。`npx skills add` 的默认目录这次没跑，不能假设它会进这个路径 |
| 项目规则 | `ProjectRulesModal` 编辑 `AGENTS.md` / `CLAUDE.md` / `.grok` 规则。文案要求先信任项目，代理才加载 | 这是项目说明，不是知识库浏览器 |
| `docs/llm-wiki/` | 给改这个仓库的代理看的产品规则，由根 `AGENTS.md` 指向 | 不要把它换成 LLM Wiki 的数据目录，也不要在这里做用户 Wiki |
| 侧栏 | `src/lib/sideWorkbench.ts`：file、browser、terminal、skills、review，另有进程创建的 plan | 没有笔记或 Wiki 页种类 |
| 记忆 | 设置 → 常规 → 代理：实验性记忆和记忆浏览器（`docs/llm-wiki/settings-ia.md`） | 会话记忆，不是 `wiki/*.md` |
| 家目录 | 共享模式 `GROK_HOME=~/.atlas`；独立模式 App `agent-home`。`atlas.md` 禁止改桌面会话、Relay、`agent serve`、SSH | 本集成也不改这些 |

`FilePathCard.tsx` 里对相对路径 `知识库/wiki/...` 的注释，只是按项目根解析路径，没有知识库产品。

## 8. 集成方案（只保留这一条）

**外置桌面应用 + 用户放进当前项目的 HTTP skill。** AtlasBuddy 不发版、不写安装器、不增加设置项。

用户自己做的事：

1. 从 nashsu 的 Releases 安装并打开 LLM Wiki 0.6.13（Eric9701 的 fork 没有额外补丁）。
2. 在 LLM Wiki 里建项目、配置它自己的模型，并在它的 API 设置里打开本机 API、生成令牌。保持默认绑定 `127.0.0.1`，不要为了 Atlas 打开局域网。
3. 若希望 Atlas 代理会用它：把 `nashsu/llm_wiki_skill` 的 `SKILL.md` 放到**该项目** `.grok/skills/llm-wiki/SKILL.md`，令牌只放在用户自己提供给该代理进程的环境里。不执行来源不明的 `npx skills add` 作为 Atlas 的安装步骤。
4. 查询用 skill 里的搜索和读文件。重扫要用户口头要求。图谱和审核仍在 LLM Wiki 里看。

这条路径不新增 `useState`，不改会话 / Relay / `agent serve` / SSH，不调用 `mcp_add`，因此不会因为这次集成去写 `~/.atlas`。

明确不要做：

- 不要把 `src-tauri`、LanceDB、pdfium、图谱、摄入队列或 LLM Wiki 的 Agent 编译进 AtlasBuddy。
- 不要把 GPL-3.0 源码，包括许可证字段写着 MIT 的 `mcp-server/`，合并进本仓库。
- 不要做 Wiki 侧栏、笔记页，或把 `docs/llm-wiki/`、记忆浏览器改成这个产品。
- 不要自动安装插件、skill 或 MCP。
- 不要把「设置 → 扩展 → MCP → 添加」当作默认步骤。那个按钮在 shared 模式会把 `[mcp_servers.*]` 写进 `~/.atlas/config.toml`，不属于 `atlas.md` 已写明的例外。
- 不要把桌面会话转发给 `llm_wiki_chat`。那是另一套带 shell 和写页面能力的 Agent。
- 不要连接剪藏端口 19827，也不要让 Atlas 去写 LLM Wiki 项目里的 `raw/` 或 `.llm-wiki/`。

stdio MCP 可以以后再考虑，而且要单独决定：是否允许现有 `mcp_add` 写共享 `config.toml`，以及 `plugin_mcp.rs` 仍只扫 `~/.grok` 时插件安装落在哪里。那不是这一条方案的一部分。

## 9. 开放问题

1. 浅克隆没有完成。同一提交的 zip 已在临时目录解压，但没有在本地 `cargo test` / `npm test`，也没有启动桌面进程对 `19828` 发请求。
2. 没有下载 Releases 安装包，不知道二进制是否等于 `e08cfce`。
3. `llm_wiki_skill` 读的是默认分支，没有和 `e08cfce` 对齐。它声称 v1 没有写接口，桌面源码里已经有写页面和聊天。
4. 没有运行 `npx skills add`，不知道它在 Windows 上写进哪个 skills 目录，以及 Atlas 的「探测 Claude/Cursor 技能」关掉时，代理是否还读得到。
5. 没有核对 Atlas 拉起代理时会继承哪些用户环境变量，因此不能保证只靠项目 skill 就能看到 `LLM_WIKI_API_TOKEN`。
6. `mcp-server/package.json` 的 MIT 和根 GPL-3.0 以哪一份为准，仓库里没有说明。
7. shared 模式下现有 MCP 添加会写 `~/.atlas/config.toml`。这和 `atlas.md`「其它写入仍禁止」之间，需要产品单独承认或改掉；本调研不把它当成可以新增的例外。
