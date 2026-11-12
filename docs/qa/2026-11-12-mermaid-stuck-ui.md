# QA · 2026-11-12 · AtlasBuddy 聊天区偶现"卡住"（Mermaid 错误图残留）

> 现场：用户截图，AtlasBuddy（白标 Grok App，`com.atlasapp.desktop` 0.2.46）聊天视图。
> 截图原始文件：会话附件 `E:\work\atlas-work\.remote\gwArZ3xsRYunZitjC9LpCgAA\img_v3_0217c_85108ec8-c32d-4fda-a0d9-b78febbf04cg.jpg`（未入库，含现场会话内容）。
> 分支：`feat-atlas`；改动文件：`src/lib/mermaidRender.ts`、`src/lib/mermaidRender.test.ts`。

## 汇总

| ID | 问题 | 现象 | 状态 |
|----|------|------|------|
| I-001 | Mermaid 解析失败后残留巨型错误图 | 聊天区被一张 `Syntax error in text / mermaid version 11.17.2` 的巨图压住，页面被撑高，滚动错位 | 已修 |
| I-002 | 流式渲染期间源码不完整会先弹错误卡片 | Agent 还在吐 mermaid 源码时，180ms 去抖会先渲染半截源码 | 已缓解，待优化 |
| I-003 | 已注册工作流 `deep-research` 指向本机不存在的路径 | 注册项写 `D:\a\grok-build\grok-build\...`，本机无 `D:\a` | 待处理（环境侧） |

---

## I-001 · Mermaid 解析失败后残留巨型错误图（已修）

### 现象

- 聊天区被一张巨大图压住，画的是 mermaid 自带的错误图：`Syntax error in text` + `mermaid version 11.17.2`。
- 该错误图按 `font-size: 150px`、`x=1440` 绘制，宽度远超会话列，页面高度被撑出约 113px，滚动位置错乱，整体观感像界面卡死。
- 偶现：只在模型输出的 mermaid 源码有语法错误或被流式截断时命中（截图那轮模型为 `cpos-deepseek4.1-flash`，会话 `SSP source`）。

### 根因

mermaid 11.x 的 `render()` 在解析失败时的顺序是：

1. `Diagram.fromText("error")` → 用 `errorRenderer.draw()` 把错误图画进临时节点 `#d<id>`（挂在 `document.body`）；
2. 记录 `parseEncounteredException`；
3. 序列化 SVG 后 `throw parseEncounteredException`。

而清理临时节点的 `removeTempElements()` 写在第 3 步的 throw **之后**，所以解析失败路径永远走不到它。桌面端 `src/lib/mermaidRender.ts` 用的是默认配置（`suppressErrorRendering` 未开），于是：

- React 侧拿到的是异常，走友好的 `chat.mermaidError` 错误卡片（这部分原本是对的）；
- 但那张 1341px 宽的错误图已经作为 `#d<id>` 留在 `document.body` 里，在 React 树之外，谁也管不到它——它压在会话上，页面还能被它撑高。

### 修复

`src/lib/mermaidRender.ts`：

- 初始化 mermaid 时加 `suppressErrorRendering: true`：解析失败直接抛错并自清临时节点，不再画错误图。
- `normalizeSvg()` 增加兜底：识别并拒绝 mermaid 的错误图 SVG（判据是文案 `Syntax error in text` / `mermaid version x.y`；**不能**用 CSS 类名 `.error-text`，每张图的样式表里都有它）。
- 渲染失败时主动移除可能残留的 `#d<id>` 节点，防止其他 mermaid 版本再泄漏。

### 验证

- 单测：`src/lib/mermaidRender.test.ts` 新增 4 条（初始化配置 / 拒绝错误图 / 含 `.error-text` 样式的正常图仍放行 / 失败后清理残留节点）。相关套件 `mermaidRender`、`MermaidBlock`、`MarkdownChat`、`MarkdownBody` 共 23 条全通过；`tsc -b` 与 eslint 无报错。
- 浏览器（无头 Chrome + Vite 临时页，真实 mermaid 11.17.2）：
  - 修复前配置复现 → `document.body` 残留 `d<id>` 节点（4487 字符），文档高度 666px → 779px；
  - 修复后 → 渲染抛错、`document.body` 无任何残留节点、页面显示 `Could not render diagram` 错误卡片与源码回退、不注入任何 SVG；
  - 正常流程图仍正常出图（12137 字符 SVG）。

### 尚未覆盖

- Tauri 应用窗口内的最终观感未在本机验证（本机没有 ego lite，`ego-browser` 命令不可用，只能用无头 Chrome 挂真实组件验证）。
- `D:\work\atlas-core\packages\artifact-viewer`（mermaid 11.16.1）**已经**带 `suppressErrorRendering: true`，同一模式在那边不存在，无需跟进。

---

## I-002 · 流式渲染期间源码不完整会先弹错误卡片（已缓解，待优化）

`MermaidBlock` 在流式阶段用 180ms 去抖触发渲染，源码尚未写完时先渲染半截内容，失败后先显示错误卡片，等完整源码到达再纠正。

现状：`streaming` 时若已有上一张成功的图会保留它，不覆盖；但**首张图**尚未成功时仍会闪一次错误卡片。

建议（未实施）：流式阶段只显示加载态，把错误卡片留给源码收尾后仍然失败的情况。

---

## I-003 · 已注册工作流 `deep-research` 指向不存在的路径（待处理，环境侧）

Atlas 已注册工作流列表里，`deep-research` 的绝对路径为
`D:\a\grok-build\grok-build\crates\codegen\xai-grok-shell\session\workflows\deep-research.rhai`，
本机 `D:\a` 整个目录不存在（`E:\GitHub\grok-build` 是另一个 Rust CLI 仓库，不是该路径）。

结果：该工作流在本机无法执行。需要确认注册来源（安装路径 / 迁移记录），把工作流注册项指向真实存在的目录，或重新安装后再注册。
