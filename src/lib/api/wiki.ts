/** Wiki engine host commands. The engine does not open a desktop session. */

import { invoke } from "./host";

export type WikiOpen = {
  project: string;
  hasSchema: boolean;
  hasPurpose: boolean;
  hasIndex: boolean;
};

export type WikiText = {
  path: string;
  content: string;
};

export type WikiPageInput = {
  path: string;
  content: string;
};

export type WikiBroken = {
  path: string;
  target: string;
};

export type WikiLint = {
  broken: WikiBroken[];
  orphans: string[];
};

export type WikiHit = {
  path: string;
  title: string;
  score: number;
  snippet: string;
};

export type WikiTokenUse = {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
};

export type WikiComplete = {
  text: string;
  modelId: string;
  usage?: WikiTokenUse | null;
};

export type WikiUsageEntry = {
  at: string;
  source: string;
  modelId: string;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  reported: boolean;
};

export function wikiOpen(project: string) {
  return invoke<WikiOpen>("wiki_open", { project });
}

export function wikiRead(project: string, path: string) {
  return invoke<WikiText>("wiki_read", { project, path });
}

export function wikiReadPages(project: string, paths: string[]) {
  return invoke<WikiText[]>("wiki_read_pages", { project, paths });
}

export function wikiWritePages(
  project: string,
  pages: WikiPageInput[],
  allowOverwrite = false,
) {
  return invoke<string[]>("wiki_write_pages", { project, pages, allowOverwrite });
}

export function wikiRemovePage(project: string, path: string) {
  return invoke<void>("wiki_remove_page", { project, path });
}

export function wikiList(project: string) {
  return invoke<string[]>("wiki_list", { project });
}

export function wikiListRaw(project: string) {
  return invoke<string[]>("wiki_list_raw", { project });
}

export function wikiListDirs(project: string) {
  return invoke<string[]>("wiki_list_dirs", { project });
}

export function wikiScaffold(parent: string, name: string, template: string) {
  return invoke<string>("wiki_scaffold", { parent, name, template });
}

export function wikiInit(project: string, template: string) {
  return invoke<string>("wiki_init", { project, template });
}

export function wikiImportFiles(project: string, paths: string[]) {
  return invoke<string[]>("wiki_import_files", { project, paths });
}

export function wikiImportUrl(project: string, url: string) {
  return invoke<string>("wiki_import_url", { project, url });
}

export function wikiRemoveRaw(project: string, path: string) {
  return invoke<void>("wiki_remove_raw", { project, path });
}

export function wikiLint(project: string) {
  return invoke<WikiLint>("wiki_lint", { project });
}

export function wikiSearch(project: string, query: string, limit?: number) {
  return invoke<WikiHit[]>("wiki_search", { project, query, limit });
}

export function wikiFingerprint(project: string, path: string) {
  return invoke<string>("wiki_fingerprint", { project, path });
}

export function wikiCacheGet(project: string, key: string) {
  return invoke<string | null>("wiki_cache_get", { project, key });
}

export function wikiCachePut(project: string, key: string, value: string) {
  return invoke<void>("wiki_cache_put", { project, key, value });
}

export function wikiComplete(prompt: string, project: string, modelId?: string) {
  return invoke<WikiComplete>("wiki_complete", {
    prompt,
    project,
    modelId: modelId ?? null,
  });
}

export function wikiUsageList(project: string) {
  return invoke<WikiUsageEntry[]>("wiki_usage_list", { project });
}

export type WikiSessionSource = {
  runtime: "atlas" | "claude" | "cursor" | "codex" | string;
  enabled: boolean;
  home: string;
};

export type WikiSessionRef = {
  runtime: string;
  id: string;
};

export type WikiSessionConfig = {
  sources: WikiSessionSource[];
  cwdPrefixes: string[];
  included: WikiSessionRef[];
  excluded: WikiSessionRef[];
  autoIncludeNew: boolean;
};

export type WikiPrompts = {
  digest: string;
  analyze: string;
  generate: string;
};

export type WikiSessionHit = {
  runtime: string;
  id: string;
  title: string;
  cwd: string;
  updatedAt: string;
  included: boolean;
};

export type WikiSessionSync = {
  written: string[];
  rollups: string[];
};

export function wikiSessionConfigGet(project: string) {
  return invoke<WikiSessionConfig>("wiki_session_config_get", { project });
}

export function wikiSessionConfigSet(project: string, config: WikiSessionConfig) {
  return invoke<WikiSessionConfig>("wiki_session_config_set", { project, config });
}

export function wikiPromptsGet(project: string) {
  return invoke<WikiPrompts>("wiki_prompts_get", { project });
}

export function wikiPromptsSet(project: string, prompts: WikiPrompts) {
  return invoke<WikiPrompts>("wiki_prompts_set", { project, prompts });
}

export function wikiSessionsScan(project: string) {
  return invoke<WikiSessionHit[]>("wiki_sessions_scan", { project });
}

export function wikiSessionsSync(project: string) {
  return invoke<WikiSessionSync>("wiki_sessions_sync", { project });
}

export function wikiUsageAppend(
  project: string,
  entry: {
    source: string;
    modelId?: string;
    inputTokens?: number;
    outputTokens?: number;
    totalTokens?: number;
    reported?: boolean;
  },
) {
  return invoke<WikiUsageEntry[]>("wiki_usage_append", {
    project,
    source: entry.source,
    modelId: entry.modelId ?? null,
    inputTokens: entry.inputTokens ?? 0,
    outputTokens: entry.outputTokens ?? 0,
    totalTokens: entry.totalTokens ?? 0,
    reported: entry.reported ?? false,
  });
}
