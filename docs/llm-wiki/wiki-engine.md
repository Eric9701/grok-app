# 知识库引擎

AtlasBuddy 里的独立引擎：把项目目录中的原文编成 `wiki/*.md`。它不打开桌面会话，不写 `~/.atlas`，也不监听 LLM Wiki 的 `19828`。

## 接口

- `wiki_open` / `wiki_read` / `wiki_write_pages` / `wiki_list` / `wiki_lint` / `wiki_search`
- `wiki_fingerprint` 与 `{知识库}/.atlas-wiki/ingest-cache.json`：原文哈希没变就先确认；确认后会忽略这条记录再编译一次，并允许覆盖已有页面
- `wiki_complete`：另起一次 Atlas headless 补全，在后台线程等待，不卡住窗口。一次编译调用两次：先分析原文，再按 `FILE` 块生成 `wiki/*.md`。编译时在页面上选择模型；没选时才用应用设置里的模型，再退到 `[models].default`。子进程的工作目录是当前知识库目录。`GROK_HOME` 用能列出所选模型的家目录：独立模式下企业模型在共享的 `~/.atlas`，不在应用的 `agent-home`。单次超过 180 秒会结束进程。提示写在临时文件里，用 `--prompt-file` 传入，避免 Windows 命令行放不下长原文。不进入正在打开的桌面会话，不改 `config.toml`，也不传 `--always-approve`

写入只接受项目内 `wiki/*.md`。一批页面有一条路径不合法，整批不落盘。默认不覆盖。`src/wiki/engine.ts` 负责两次提示和 `FILE` 块解析，模型通过 `complete` 注入。每次编译会在智能体看板上放一张知识库卡片，点击后可以看到读取、分析、生成、写入的进度。生成回复如果没有 `FILE` 块，会再要一次；仍然没有时，把正文存成 `wiki/sources/<原文名>.md`。后一页以新的 `---FILE:` 开头时，即使中间漏了 `---END FILE---` 也会切开。这次写出了正式页面后，删掉同一篇原文上次留下的 `wiki/sources/` 占位页。

检索目前是关键词，不含向量库。侧栏只有一个入口「个人知识库」（`#/wiki`），页面只展示设置里配置的目录。编译原文从该目录的 `raw/` 里选择，不手填路径。设置 → 通用 → Agent 可以添加目录，并指定哪一个是 Agent 默认；新会话会带上这个目录，不写 `config.toml`。每次编译如果模型返回了 token 用量，会记在该目录的 `.atlas-wiki/usage.json`，并在页面上查看。PDF 和 Office 解析仍不在这一期。
