/** Binds the wiki engine to Atlas host commands. */

import * as api from "@/lib/api";
import {
  ingest,
  type Complete,
  type IngestResult,
  type IngestStage,
  type WikiIo,
} from "./engine";

export const wikiIo: WikiIo = {
  async read(project, path) {
    const text = await api.wikiRead(project, path);
    return text.content;
  },
  writePages(project, pages, allowOverwrite) {
    return api.wikiWritePages(project, pages, allowOverwrite);
  },
  fingerprint(project, path) {
    return api.wikiFingerprint(project, path);
  },
  cacheGet(project, key) {
    return api.wikiCacheGet(project, key);
  },
  cachePut(project, key, value) {
    return api.wikiCachePut(project, key, value);
  },
  remove(project, path) {
    return api.wikiRemovePage(project, path);
  },
};

/** One-shot Atlas completion. The child cwd is the knowledge base. */
export function atlasComplete(project: string, modelId?: string): Complete {
  return async (prompt) => {
    const result = await api.wikiComplete(prompt, project, modelId);
    return {
      text: result.text,
      usage: result.usage,
      modelId: result.modelId,
    };
  };
}

export async function ingestWithAtlas(
  project: string,
  sourcePath: string,
  modelId?: string,
  onProgress?: (stage: IngestStage) => void,
  force?: boolean,
): Promise<IngestResult> {
  const result = await ingest({
    project,
    sourcePath,
    complete: atlasComplete(project, modelId),
    io: wikiIo,
    onProgress,
    force,
  });
  if (!result.skipped) {
    try {
      await api.wikiUsageAppend(project, {
        source: sourcePath,
        modelId: result.modelId,
        inputTokens: result.usage?.inputTokens ?? 0,
        outputTokens: result.usage?.outputTokens ?? 0,
        totalTokens: result.usage?.totalTokens ?? 0,
        reported: result.usage != null,
      });
    } catch {
      /* The pages are already written. The usage list can refresh later. */
    }
  }
  return result;
}
