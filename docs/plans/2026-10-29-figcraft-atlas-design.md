# 方案：Atlas 里编辑 UI/UE（FigCraft 外挂，不进主仓）

| 字段 | 值 |
|------|------|
| **状态** | 方案，未开工 |
| **日期** | 2026-10-29 |
| **上游** | [storytold/figcraft](https://github.com/storytold/figcraft) `0.12.1`，MIT OR Apache-2.0 |
| **用户给出的仓库** | [Eric9701/figcraft](https://github.com/Eric9701/figcraft)（2026-10-29 当天 fork，无自有提交，落后上游） |
| **产品** | Atlas 白标桌面应用里打开并编辑界面稿 / 体验稿 |
| **本文范围** | 边界、分期、不做的事。不改代码 |

这里的 **UI/UE** 指界面与体验稿：画板、自动布局、组件与变体、变量、原型、Inspect 出码。不是 Unreal Engine，也不是侧栏浏览器里已有的「设计模式」（那个只检查正在跑的网页 DOM，把选中元素送进对话）。

## 结论

FigCraft 已经按「引擎 / 命令 / 可替换 UI」拆开，并且**网页版明确写了给别的应用用 iframe / 控制通道嵌进去**。Atlas 只做宿主：钉住一份官方网页构建，放进现有的侧栏子 WebView，用它自己的画布编辑；智能体走它自己的 MCP，经一条很薄的本机转发接到同一份文档。

不要把 `figcraft-*` crate 编进 `src-tauri`，不要在 React 里重写画布，不要以 Eric9701 的 fork 当集成分支。

## 上游是什么

[storytold/figcraft](https://github.com/storytold/figcraft) 是纯 Rust、洁净室实现的界面设计与原型工具（对标公开的 Figma 工作流，不读对方安装包）。桌面、命令行、浏览器 WASM、协作服务器四条入口。`cargo xtask layers` 在编译期锁分层：几何与颜色 → 文档 → 布局 / 文本 / 路径 / 插件 / 协作 → 渲染 / 格式 → SVG / PDF → 引擎 → egui UI 与 MCP → 应用。**L6 以下不依赖 egui。** 菜单、快捷键、CLI、控制通道、MCP、WASM 插件都调用同一套命令 id（文档写明命令注册表在 400 条以上；架构文档在 #101 之后记过 455 个 id）。

原生文件是 `.figcraft`（zip：`document.json` + 图片）。另可读写 SVG、图片、实验性 `.fig`、Sketch、Penpot、REST API JSON。Dev Mode 能把图层写成 React、Tailwind、Vue、Leptos、SwiftUI、Compose、ArkUI、Flutter，并用 `figcraft.connect.json` 把组件接到仓库里的真实组件。

许可是 MIT OR Apache-2.0。`docs/brand/` 里的标志是商标，不能改、不能挪到 Atlas 图标。字体在另一个仓库，构建可以不带它（运行时扫系统字体）。

### Eric9701/figcraft

GitHub API（2026-10-29）：`fork: true`，`parent` 为 `storytold/figcraft`，创建时间 `2026-10-29T07:42:42Z`。该 fork 最近提交是上游的 Penpot 导入合并 `fe7b4bac`（`2026-10-29T07:20:14Z`）。同一时刻上游 `main` 已是 `3af5d8a5`（评论名单修复）。没有 Atlas 向的提交。后续钉版本只跟踪 storytold 的 tag / commit，不维护这份 fork。

### 和 Atlas 工具链不兼容（所以不能源码合入）

| 项 | FigCraft | Atlas（本仓 `src-tauri`） |
|----|----------|---------------------------|
| edition | 2024 | 2021 |
| rust-version | 1.95 | 跟随本仓现有工具链 |
| UI | egui 0.37，且 `[patch]` 到 storytold/egui、storytold/winit | Tauri + React |
| 体量 | 架构文档记约 10.8 万行 Rust；网页构建有 22 MiB 量级的 wasm 门禁 | 壳与 AppWorkbench 只允许减行 |

合入会把第二套 GUI、第二套工具链和一份商标资产带进白标应用。分层文档自己也说 UI crate 可替换，但替换成本是整份设计器，不是 Atlas 该做的事。

## 已经存在的嵌入口

依据 `apps/figcraft-web`（`main` 上的 `query.rs`、`api.rs`）：

1. **页面查询 `?control`** 才安装 `window.figcraft`。不带这个参数就没有控制面。
2. **`window.figcraft` 与桌面控制通道是同一套 JSON 方法**，注释写明给「浏览器标签，或另一个应用里的 iframe」用：
   - `execute(command, params)` → `engine.execute`
   - `request(method, params)` → 控制方法
   - `openFile(name, bytes)` → 用字节打开文件
   - `captureDownloads(true)` 后，保存留在页面里，再用 `files()` / `readFile(name)` 读回
   - `screenshot()` → 画布 PNG
3. **邀请链接放在 hash**：`#join=<link>`。fragment 不出浏览器，密钥不进宿主访问日志。网页版可以加入协作，不能当主机。
4. 桌面侧等价物是 `figcraft --control <port>`（只听 `127.0.0.1`）和 `figcraft-cli mcp` / `figcraft-cli mcp --attach <port>`。`--attach` 时用户能看见每次修改。文件范围用 `--automation-read-root` / `--automation-write-root`，根必须是已存在的目录；`--attach` 与这两个旗标不能同时出现在 CLI 上，旗标要加在**真正执行命令的那一端**。

MCP 工具面（stdio，协议 `2025-06-18`）包括 `run_command`、`create_node`、`set_properties`、`inspect_document`、`export`、`screenshot`、`open_file`、`save_file`，以及原型、组件、变量、Dev Mode、`collab.*`。智能体不需要 Atlas 再包一层「创建设计」API。

## 目标形态

用户在 Atlas 侧栏打开一份 `.figcraft`（或从空白模板新建），**画布、图层面板、属性、原型、Inspect 都是 FigCraft 自己的界面**，出现在工作台右侧，而不是另一个操作系统窗口。对话里的智能体改的是这一份正在看的文档。导出的代码、SVG、PNG 落在当前项目目录，用现有文件标签打开。

Atlas 自己的界面只保留标签标题、路径、未保存提示、打开失败、以及「用系统应用打开」。FigCraft 窗口里的菜单和文案走它自己的语言包（含简体中文草稿），Atlas 不去翻译、不去换肤。

## 边界（四条，互不拥有对方的数据）

```text
项目目录 .figcraft / 导出物
        │ 字节进、字节出
        ▼
Atlas 设计宿主（新模块）
  · 本机静态页：钉住的 figcraft-web
  · 子 WebView（沿用侧栏浏览器那条）
  · 一条 127.0.0.1 控制转发：JSON 行 ↔ window.figcraft
        │ 同一份文档
        ▼
FigCraft 网页引擎（上游二进制，Atlas 不编译）
        ▲
        │ stdio MCP --attach
智能体（现有 mcpServers 注入，不改 ~/.atlas）
```

| 谁 | 拥有 | 不拥有 |
|----|------|--------|
| FigCraft 网页构建 | 文档模型、撤销、渲染、面板、命令 id、插件沙箱 | Atlas 设置、会话、项目信任 |
| Atlas 设计宿主 | 标签、本机端口、把项目文件字节送进页面、把页面里的保存写回项目、把控制通道转成 MCP 能 `--attach` 的端口 | 图层语义、坐标、出码规则 |
| 智能体 | 调用上游命令与 Dev Mode | 直接改 WebView DOM，或另开一份无窗口文档去改同一文件 |
| 项目目录 | `.figcraft` 与导出文件 | 编辑器偏好、协作密钥 |

转发器只做运输：一行 JSON 进去，`figcraft.request` 的结果一行 JSON 出来。形状以 `docs/control-protocol.md` 为准。不在 Atlas 里登记 400 条命令，也不缓存文档 JSON。

密钥与邀请链接只留在内存和页面 hash 里，不进日志、不进会话转录、不进 `~/.atlas`。

## 为什么不用另外三种接法

| 接法 | 结果 |
|------|------|
| 源码合进 `src-tauri` | 工具链、egui patch、体积和壳行数都不成立 |
| 只启动独立的 FigCraft 窗口 | 能编辑，但不在 Atlas 里 |
| 无窗口 `figcraft-cli mcp` 改磁盘，网页再重新打开 | 两份文档，智能体改的和人眼看见的会分叉 |
| 把 egui 窗口重新父级到 Atlas | Windows DPI、焦点、标题栏都会碎，而且绑死桌面后端 |
| 用协作把「无窗口主机 + 网页客人」接起来 | 上游支持，但是多一个进程和同步延迟。等单页转发不够用再考虑。网页版不能当主机，所以主机仍得是 CLI 或桌面进程 |

单页转发让**人看见的那份页面就是文档主人**。`figcraft-cli mcp --attach` 接到转发端口后，走的仍是上游 MCP，Atlas 不实现工具 schema。

没有打开设计标签时，允许第二条、互斥的路径：对**尚未在标签里打开**的文件，用带读写根（仅当前项目）的无窗口 MCP 改磁盘。一旦该文件在标签里打开，这条无窗口会话必须先停，改由 `--attach`。同一路径不能两头写。

## 和现有 Atlas 表面的关系

| 现有表面 | 关系 |
|----------|------|
| 侧栏 `file` / `browser` / `terminal` / `review` / `skills`（`src/lib/sideWorkbench.ts`） | 新增一种标签 `design`。选择器里单独一项。不占用浏览器标签 |
| 侧栏浏览器「设计模式」（`useBrowserDesignMode`） | 保留。它检查 localhost 页面的 DOM。设计稿标签不注入那段脚本 |
| `side_browser_host` 子 WebView | 设计标签复用「带 label 的子 WebView + 页内 eval」。静态资源用设计宿主自己的 loopback，不把 wasm 塞进主窗口 |
| 资源预览（`ResourcePreviewBody`） | `.figcraft` 不进文本编辑器。打开动作切到设计标签 |
| 插件市场 / ChatCut | 不把 FigCraft 装成 grok plugin。它不是 skill 包，是外部编辑器 |
| 外观包 `.grokskin` | 无关。设计稿不写入皮肤 |
| `App.tsx` / `AppWorkbench.tsx` | 不新增 state。状态进 `src/lib/designHost.ts` 与 `src/components/design-surface/`，进程与静态服务进 `src-tauri/src/design_host/` |
| 共享 `~/.atlas` | 不写。二进制路径与开关进 App 设置；MCP 只在会话 `mcpServers` 里注入，和 pr-monitor 一样不改 CLI 家目录 |
| 文案 | Atlas 外壳字符串进 `src/i18n/`，十五个目录与 `en` 对齐。FigCraft 内部文案不动 |

设置要登记 `settingsCatalog`（建议 `design.figcraft`：启用、网页构建目录或已确认的版本钉、CLI 路径）。安装与切换构建必须走应用内确认，不自动下载、不静默替换正在打开的文档。

深链与快捷键后做。第一期从文件树、资源预览和侧栏「+」进入即可。

## 打开与保存

1. 用户打开 `app.figcraft` 或新建。宿主确保钉住的网页构建已在 `127.0.0.1` 上，地址形如 `http://127.0.0.1:<port>/?control`（不要把文件字节放进 query）。
2. 页面发出 `window.figcraft` 之后，宿主调用 `openFile(文件名, 字节)`。新建则 `execute("file.newFromTemplate", …)` 或空文档，模板 id 用上游的 `mobile-app`、`landing-page`、`dashboard` 等，不在 Atlas 再做一套模板。
3. 打开时 `captureDownloads(true)`。用户在 FigCraft 里保存后，宿主 `readFile` 写回项目路径。写回前如果磁盘上的文件比打开时新，先问用户（应用内对话框，不用 `window.confirm`）。
4. 标签关闭：有未读回的保存则提示。转发端口关掉，`--attach` 的 MCP 下一调用得到「没发出去」，与上游对丢失回复的约定一致。
5. 「用系统应用打开」启动用户自己的 `figcraft` 桌面版并带上同一路径。此时关掉 Atlas 里的这份标签，避免双写。

网页构建大约二十兆，第一次加载要有明确的加载与失败状态（构建缺失、WebGL 不可用、端口被占、文件打不开）。失败时标签留在原地并说明原因，不退成文本预览。

键盘：焦点在画布里时，FigCraft 处理自己的快捷键；Atlas 的全局快捷键不要在这块子 WebView 上抢 Ctrl+Z / 删除 / 方向键。这是宿主要测的行为，不是要改上游键位。

## 智能体

设计标签处于打开状态时，会话注入的 MCP 是：

```json
{
  "mcpServers": {
    "figcraft": {
      "command": "<钉住的 figcraft-cli>",
      "args": ["mcp", "--attach", "<该标签的转发端口>"]
    }
  }
}
```

读写根加在**页面所代表的那一端**（转发器执行命令的地方）：只允许当前项目目录。相对路径按上游规则解析后再比对，拒绝项目外路径。导出 PNG / SVG / 代码片段若走 MCP 且不带 `path`，字节回到工具结果，由智能体再用现有文件工具写入仓库。

出码是第二期，仍然只调用上游命令，例如 `inspect.code` 的 `language: "tailwind" | "react" | "vue" | …`，以及 `codeConnect.import` 读取仓库里的 `figcraft.connect.json`。Atlas 不解析图层、不生成组件。

原型播放留在 FigCraft 画布里。做完的前端若要对照实现，仍用现有浏览器标签和设计模式，两套表面不合并。

## 分期

### 0. 钉版本与探测（不出现编辑界面）

- 记录 commit、`figcraft-web` 构建物、与之匹配的 `figcraft-cli`。三者同一 commit。
- 设置里探测 CLI 与构建目录：缺失、版本不一致、不是文件，分开说。不自动安装。
- 用一条最小测试确认：`?control` 之后 `execute("shape.rectangle", …)` 有返回，`openFile` + `captureDownloads` + 保存能读回字节。协议以 `docs/control-protocol.md` 的请求/响应样例为准，转发器对照样例，不凭记忆写。

### 1. 侧栏里能编辑

- 标签种类 `design`，打开 / 新建 / 保存回项目 / 关闭提示 / 失败状态。
- 本机静态服务 + 控制转发 + 子 WebView。
- 会话 MCP `--attach` 到当前设计标签。无标签时的无窗口路径与之互斥。
- 十五种语言只覆盖 Atlas 外壳。
- 验收：在 Atlas 里拖出一个画板并改填充；对话里让智能体加一个文本图层，画布上立刻出现；保存后项目里的 `.figcraft` 用官方 `figcraft-cli info` 能读到该图层。

### 2. 交接到代码

- 智能体把 `inspect.code` / `export` 的结果写入项目，现有文件标签打开。
- 可选：把 `figcraft.connect.json` 当普通文件，不另做编辑器。
- 资源树对 `.figcraft` 显示「在设计页打开」，而不是代码预览。

### 3. 再评估，不预做

- 本地 `collab.host`（`bind: local`）只在转发器证明不够（例如要桌面版与 Atlas 同时改）时再做。
- 不自建 `figcraft-server`，不把稿子发到公网。上游写明：发布到别人的服务器必须由用户明确要求。
- 插件仍在 FigCraft 里安装（wasmi，无文件与网络）。Atlas 插件市场不列出它们。
- vectorcraft / photocraft 等同族应用若要嵌，复用同一宿主与 `?control`，另开方案。

## 明确不做

- 不 fork、不改 FigCraft 的画布、面板、命令和文件格式。
- 不把 egui 链进 Tauri，不用 React 重写设计器。
- 不把 Eric9701/figcraft 当成产品分支。
- 不在启动时下载构建物，不把构建物提交进 git。
- 不把设计文档、控制端口、邀请 token 写入共享 CLI 家目录或会话日志。
- 不自动把设计稿套到 Atlas 自己的界面上。
- 不做 Unreal / UMG 管线。Android Vector Drawable、SwiftUI、Compose、Flutter 只作为上游导出格式，由智能体按需调用。
- 不把「设计模式」改名为设计稿，或反过来。

## 风险

| 风险 | 处理 |
|------|------|
| WebView2 上 WebGPU 失败 | 上游已有 WebGL2 回退。第一期在 Windows 上用真实子 WebView 打开钉住的构建，确认画布出像素。只有回退也失败才阻塞，不提前换渲染后端 |
| wasm 体积与首屏 | 加载态写清楚。构建按版本缓存。切换版本时关掉已开标签 |
| 转发器与 CLI 版本错位 | 同一 commit 才允许 `--attach`。不一致就停在探测页 |
| 保存竞态 | 以打开时的文件摘要为基准，磁盘变了先问 |
| 快捷键被 Atlas 吃掉 | 第 1 期验收包含撤销、删除、方向键、文字输入 |
| 子 WebView 与主窗口焦点 | 沿用侧栏浏览器已有的 label 与显示/隐藏，不新开顶级窗口 |
| 协作密钥泄漏 | 第一期不做协作。若做，链接只放 hash |
| 许可证与商标 | 随构建物带上 LICENSE 与 NOTICE。不用 `docs/brand/` 的标志做 Atlas 图标 |

## 来源

下列行为描述均来自 2026-10-29 读取的上游文件（`storytold/figcraft` `main`），不是二手转述：

- 仓库关系与时间：GitHub API `repos/Eric9701/figcraft`、`repos/storytold/figcraft`、两边的 commit
- 分层、文件格式、命令单路径：`docs/architecture.md`、根 `AGENTS.md`、根 `Cargo.toml`（`0.12.1`，edition 2024，`rust-version = "1.95"`）
- MCP 工具、`--attach`、读写根、无窗口与附着的区别：`docs/mcp.md`
- CLI 子命令与导入导出：`docs/cli.md`
- 插件沙箱：`docs/plugins.md`
- 网页嵌入口：`apps/figcraft-web/index.html`、`src/query.rs`、`src/api.rs`、`Cargo.toml`（WebGPU，WebGL2 回退）

Atlas 侧对照：`docs/llm-wiki/atlas.md`（白标不改 shared CLI home）、`src/lib/sideWorkbench.ts`、`src-tauri/src/side_browser_host.rs`、`src/components/side-workbench/BrowserTab.tsx`（现有设计模式）、`docs/llm-wiki/plugins-marketplace.md`。
