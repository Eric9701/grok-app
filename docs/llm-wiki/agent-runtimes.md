# 多智能体 CLI 运行时

桌面会话和 Atlas 云端都可以选用本机已安装的 CLI。对外只说 ACP。Atlas 是字节透传。Claude Code 与 Cursor Agent 在 Host 模块内把各自的 stream-json 收成 ACP 行。

配置文件是应用数据目录下的 `agent-runtimes.json`。它不写入 App 设置，不写入会话元数据，也不改 `~/.atlas/config.toml`。

## 怎么选

未添加任何运行时时，桌面和云端都继续用现在的 Atlas，Composer 上不出现选择。

1. **新建会话默认**：设置 → 智能体 CLI。留空表示新聊天走现有 Atlas。选定后，新建会话会带上这个运行时。
2. **当前这条桌面会话**：Composer 里、模型芯片旁边。只改这一条。正在输出或等待权限时不能改。改之前会确认：CLI 侧上下文会丢掉，本应用里的聊天记录还在。下次连接才换进程。
3. **某一条 Atlas 云端 Agent**：同一设置页里，每条中继一个「执行运行时」。留空则这条仍用现在的本机 Atlas。改选只重连这一条。

## 绑定之后

- 桌面连接查 `sessionBindings`。有绑定就拉起该 profile 的 ACP 管道，现有会话管理器继续处理流式、权限、取消和本地记录。
- 没有绑定的会话仍走原来的 Atlas 拉起（含手动 CLI 路径、数据模式、SSH / WSL）。
- Claude / Cursor 的续聊 id 记在绑定上，不写入会话元数据里的 Atlas 会话 id。这条进程只服务一个会话，带 sessionId 的流式更新仍进入本会话。
- 非 Atlas 的会话菜单里不提供 rewind 和 fork。模型芯片只显示该 profile 的模型，不打开官方模型目录。工作目录用当前聊天的文件夹；家目录只作为 CLI 配置目录。
- SSH 文件夹上的已绑定会话会连接失败，并说明这条 CLI 只在本机运行。

云端未绑定的中继仍直接拉起 `atlas agent --always-approve stdio`。绑定之后换成同一条 ACP 管道，看板仍按 `session/update` 折叠。
