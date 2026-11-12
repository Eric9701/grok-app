/** Wiki ingest orchestration. The model is injected; this file never writes. */

import {
  buildIngestLog,
  isAggregateWikiPath,
  recentPageLines,
  updateRecentIndex,
  updateRecentOverview,
  wikiDate,
} from "./aggregates";

export type WikiPage = {
  path: string;
  content: string;
};

export type TokenUse = {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
};

export type Completion = {
  text: string;
  usage?: TokenUse | null;
  modelId?: string;
};

export type Complete = (prompt: string) => Promise<string | Completion>;

export type WikiIo = {
  read(project: string, path: string): Promise<string>;
  writePages(
    project: string,
    pages: WikiPage[],
    allowOverwrite: boolean,
  ): Promise<string[]>;
  fingerprint(project: string, path: string): Promise<string>;
  cacheGet(project: string, key: string): Promise<string | null>;
  cachePut(project: string, key: string, value: string): Promise<void>;
  remove?(project: string, path: string): Promise<void>;
};

export type IngestResult = {
  skipped: boolean;
  written: string[];
  usage: TokenUse | null;
  modelId: string;
};

const FILE_HEADER = /---\s*FILE:\s*([^\n]+?)\s*---\s*\n/gi;

const tails = new Map<string, Promise<unknown>>();

export function enqueue<T>(project: string, job: () => Promise<T>): Promise<T> {
  const prev = tails.get(project) ?? Promise.resolve();
  const run = prev.then(job, job);
  tails.set(
    project,
    run.then(
      () => undefined,
      () => undefined,
    ),
  );
  return run;
}

export function pagePathOk(raw: string): boolean {
  const text = raw.trim().replace(/\\/g, "/");
  if (!text.startsWith("wiki/") || !text.endsWith(".md")) return false;
  return text.split("/").every((segment) => segment && segment !== "." && segment !== ".." && !segment.startsWith("."));
}

export function parseFileBlocks(text: string): WikiPage[] {
  const normalized = text
    .replace(/\r\n/g, "\n")
    .replace(/```[a-zA-Z0-9_-]*\n([\s\S]*?)```/g, "$1");
  const marks: { path: string; headerAt: number; bodyAt: number }[] = [];
  for (const match of normalized.matchAll(FILE_HEADER)) {
    if (match.index == null) continue;
    const path = (match[1] ?? "")
      .trim()
      .replace(/\\/g, "/")
      .replace(/^["'`]+|["'`]+$/g, "");
    if (!path) continue;
    marks.push({
      path,
      headerAt: match.index,
      bodyAt: match.index + match[0].length,
    });
  }
  return marks.map((mark, index) => {
    const end = index + 1 < marks.length ? marks[index + 1].headerAt : normalized.length;
    let content = normalized.slice(mark.bodyAt, end);
    const closed = content.search(/---\s*END FILE\s*---/i);
    if (closed >= 0) content = content.slice(0, closed);
    return { path: mark.path, content };
  });
}

export function sourceStem(sourcePath: string): string {
  const base = sourcePath.trim().replace(/\\/g, "/").split("/").pop() || "source";
  const stem = base.replace(/\.[^.]+$/, "");
  const safe = stem
    .replace(/[^\p{L}\p{N}._-]+/gu, "-")
    .replace(/-+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60);
  return safe || "source";
}

/** Keep a prose reply when the model never emitted a FILE block. */
export function fallbackWikiPage(sourcePath: string, body: string): WikiPage | null {
  const text = body.replace(/\r\n/g, "\n").trim();
  if (text.length < 40) return null;
  if (
    /max turns reached|wiki_no_pages|wiki_complete|unknown model id|Couldn't set model/i.test(
      text,
    )
  ) {
    return null;
  }
  const path = `wiki/sources/${sourceStem(sourcePath)}.md`;
  if (!pagePathOk(path)) return null;
  return { path, content: text.endsWith("\n") ? text : `${text}\n` };
}

export function buildAnalysisPrompt(input: {
  sourcePath: string;
  source: string;
  schema: string;
  purpose: string;
  index: string;
}): string {
  return [
    "You analyze one source for a wiki. Reply with text only. Do not create or edit files.",
    "",
    "Purpose:",
    input.purpose || "(none)",
    "",
    "Schema:",
    input.schema || "(none)",
    "",
    "Index:",
    input.index || "(none)",
    "",
    `Source path: ${input.sourcePath}`,
    input.source,
  ].join("\n");
}

export function buildGenerationPrompt(input: {
  sourcePath: string;
  source: string;
  schema: string;
  analysis: string;
}): string {
  return [
    "Write wiki pages from the analysis. Reply with text only. Do not call tools. Do not create or edit files.",
    "The entire reply is one or more FILE blocks and nothing else. No introduction.",
    "Each page must use this exact fence:",
    "---FILE: wiki/concepts/example.md---",
    "page body",
    "---END FILE---",
    "Paths stay under wiki/ and end in .md.",
    "Do not emit wiki/index.md, wiki/overview.md, or wiki/log.md.",
    "",
    "Schema:",
    input.schema || "(none)",
    "",
    "Analysis:",
    input.analysis,
    "",
    `Source path: ${input.sourcePath}`,
    input.source,
  ].join("\n");
}

export function buildRepairPrompt(input: { sourcePath: string; draft: string }): string {
  const draft = input.draft.replace(/\r\n/g, "\n").trim().slice(0, 8000);
  return [
    "The previous reply had no wiki FILE block. Reply again with only FILE blocks.",
    "Do not explain. Do not create or edit files.",
    "---FILE: wiki/concepts/example.md---",
    "page body",
    "---END FILE---",
    "Paths stay under wiki/ and end in .md.",
    "",
    `Source path: ${input.sourcePath}`,
    "",
    "Previous reply:",
    draft || "(empty)",
  ].join("\n");
}

async function readOptional(io: WikiIo, project: string, path: string): Promise<string> {
  try {
    return await io.read(project, path);
  } catch (err) {
    const code = err instanceof Error ? err.message : String(err);
    if (code.includes("wiki_missing")) return "";
    throw err;
  }
}

export type IngestStage = "read" | "analyze" | "generate" | "write";

export function ingest(args: {
  project: string;
  sourcePath: string;
  complete: Complete;
  io: WikiIo;
  onProgress?: (stage: IngestStage) => void;
  force?: boolean;
}): Promise<IngestResult> {
  return enqueue(args.project, () => runIngest(args));
}

async function runIngest(args: {
  project: string;
  sourcePath: string;
  complete: Complete;
  io: WikiIo;
  onProgress?: (stage: IngestStage) => void;
  force?: boolean;
}): Promise<IngestResult> {
  args.onProgress?.("read");
  const fingerprint = await args.io.fingerprint(args.project, args.sourcePath);
  const cached = await args.io.cacheGet(args.project, args.sourcePath);
  if (!args.force && cached === fingerprint) {
    return { skipped: true, written: [], usage: null, modelId: "" };
  }
  const [schema, purpose, index, source] = await Promise.all([
    readOptional(args.io, args.project, "wiki/schema.md"),
    readOptional(args.io, args.project, "wiki/purpose.md"),
    readOptional(args.io, args.project, "wiki/index.md"),
    args.io.read(args.project, args.sourcePath),
  ]);
  args.onProgress?.("analyze");
  const analysis = completionOf(
    await args.complete(
      buildAnalysisPrompt({
        sourcePath: args.sourcePath,
        source,
        schema,
        purpose,
        index,
      }),
    ),
  );
  args.onProgress?.("generate");
  let generated = completionOf(
    await args.complete(
      buildGenerationPrompt({
        sourcePath: args.sourcePath,
        source,
        schema,
        analysis: analysis.text,
      }),
    ),
  );
  let pages = parseFileBlocks(generated.text);
  if (pages.length === 0) {
    const repaired = completionOf(
      await args.complete(
        buildRepairPrompt({
          sourcePath: args.sourcePath,
          draft: generated.text,
        }),
      ),
    );
    const repairedPages = parseFileBlocks(repaired.text);
    generated = {
      text: repairedPages.length > 0 ? repaired.text : generated.text,
      usage: addUsage(generated.usage, repaired.usage),
      modelId: repaired.modelId || generated.modelId,
    };
    pages = repairedPages;
  }
  pages = pages.filter((page) => !isAggregateWikiPath(page.path));
  if (pages.length === 0) {
    const one = fallbackWikiPage(args.sourcePath, generated.text);
    if (one) pages = [one];
  }
  if (pages.length === 0) throw new Error("wiki_no_pages");
  const seen = new Set<string>();
  for (const page of pages) {
    if (!pagePathOk(page.path)) throw new Error("wiki_path");
    if (seen.has(page.path)) throw new Error("wiki_duplicate");
    seen.add(page.path);
  }
  args.onProgress?.("write");
  const written = await args.io.writePages(args.project, pages, args.force === true);
  await writeAggregates(args, pages.filter((page) => written.includes(page.path)));
  const fallbackPath = `wiki/sources/${sourceStem(args.sourcePath)}.md`;
  if (!written.includes(fallbackPath)) {
    await args.io.remove?.(args.project, fallbackPath);
  }
  await args.io.cachePut(args.project, args.sourcePath, fingerprint);
  return {
    skipped: false,
    written,
    usage: addUsage(analysis.usage, generated.usage),
    modelId: generated.modelId || analysis.modelId || "",
  };
}

async function writeAggregates(
  args: {
    project: string;
    sourcePath: string;
    io: WikiIo;
  },
  pages: WikiPage[],
): Promise<void> {
  if (pages.length === 0) return;
  const [index, overview, log] = await Promise.all([
    readOptional(args.io, args.project, "wiki/index.md"),
    readOptional(args.io, args.project, "wiki/overview.md"),
    readOptional(args.io, args.project, "wiki/log.md"),
  ]);
  const lines = recentPageLines(index, pages);
  const next: WikiPage[] = [
    {
      path: "wiki/log.md",
      content: buildIngestLog(log, args.sourcePath, wikiDate()),
    },
  ];
  if (lines.length > 0) {
    next.unshift(
      { path: "wiki/index.md", content: updateRecentIndex(index, lines) },
      { path: "wiki/overview.md", content: updateRecentOverview(overview, lines) },
    );
  }
  await args.io.writePages(args.project, next, true);
}

function completionOf(value: string | Completion): Completion {
  return typeof value === "string" ? { text: value } : value;
}

function addUsage(
  left: TokenUse | null | undefined,
  right: TokenUse | null | undefined,
): TokenUse | null {
  if (!left && !right) return null;
  return {
    inputTokens: (left?.inputTokens ?? 0) + (right?.inputTokens ?? 0),
    outputTokens: (left?.outputTokens ?? 0) + (right?.outputTokens ?? 0),
    totalTokens: (left?.totalTokens ?? 0) + (right?.totalTokens ?? 0),
  };
}
