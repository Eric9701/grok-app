import type { SettingsEntry } from "../types";

/** Settings → Database. */
export const DATABASE_ENTRIES: readonly SettingsEntry[] = [
  {
    id: "database.dataDirMode",
    section: "database",
    anchorId: "settings-anchor-databaseDataDir",
    labelKey: "settings.database.dataDir",
    descKeys: [
      "settings.database.dataDirDesc",
      "settings.database.dataDirShared",
      "settings.database.dataDirIsolated",
    ],
    keywords: ["tabularis", "database", "data dir", "数据库", "資料庫"],
  },
  {
    id: "database.mcpEnabled",
    section: "database",
    anchorId: "settings-anchor-databaseMcp",
    labelKey: "settings.database.mcp",
    descKeys: ["settings.database.mcpDesc"],
    keywords: ["mcp", "sql", "tabularis", "agent", "数据库"],
  },
  {
    id: "database.binaryPath",
    section: "database",
    anchorId: "settings-anchor-databaseBinary",
    labelKey: "settings.database.binary",
    descKeys: ["settings.database.binaryDesc", "settings.database.binaryPlaceholder"],
    keywords: ["binary", "path", "tabularis", "程序"],
  },
  {
    id: "database.serviceStatus",
    section: "database",
    anchorId: "settings-anchor-databaseStatus",
    labelKey: "settings.database.status",
    descKeys: [
      "settings.database.running",
      "settings.database.stopped",
      "settings.database.starting",
      "settings.database.missing",
      "settings.database.restart",
      "settings.database.stop",
    ],
    keywords: ["status", "restart", "service", "tabularis", "服务"],
  },
];
