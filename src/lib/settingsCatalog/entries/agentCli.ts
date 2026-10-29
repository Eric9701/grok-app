import type { SettingsEntry } from "../types";

/** Settings → Agent CLIs. Independent of Atlas relay records and App settings. */
export const AGENT_CLI_ENTRIES: readonly SettingsEntry[] = [
  {
    id: "agentCli.profiles",
    section: "agent_cli",
    anchorId: "settings-anchor-agentCliProfiles",
    labelKey: "settings.agentCli.profiles",
    descKeys: ["settings.agentCli.intro"],
    keywords: [
      "claude",
      "cursor",
      "agent cli",
      "runtime",
      "智能体",
      "多智能体",
      "命令行",
    ],
  },
  {
    id: "agentCli.default",
    section: "agent_cli",
    anchorId: "settings-anchor-agentCliDefault",
    labelKey: "settings.agentCli.default",
    descKeys: ["settings.agentCli.defaultEmpty"],
    keywords: ["default cli", "new chat", "新建会话", "默认"],
  },
];
