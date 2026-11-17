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

export type WikiPromptSet = {
  digest?: string;
  analyze?: string;
  generate?: string;
};

/** Always appended after a custom or built-in generation prompt. */
export const LOCKED_GENERATION_RULES = [
  "Reply with text only. Do not call tools. Do not create or edit files.",
  "The entire reply is one or more FILE blocks and nothing else. No introduction.",
  "Each page must use this exact fence:",
  "---FILE: wiki/concepts/example.md---",
  "page body",
  "---END FILE---",
  "Paths stay under wiki/ and end in .md.",
  "Do not emit wiki/index.md, wiki/overview.md, or wiki/log.md.",
  "Each page starts with frontmatter containing title, type, sources (the raw source path), and related.",
  "In the body, link an existing page as [[concepts/name]], using its path under wiki/ without the wiki/ prefix or .md.",
].join("\n");

export const LOCKED_ANALYSIS_RULES =
  "Reply with text only. Do not call tools. Do not create or edit files.";

export function promptRevision(prompts: WikiPromptSet | undefined): string {
  const text = [prompts?.digest ?? "", prompts?.analyze ?? "", prompts?.generate ?? ""].join("\n\0");
  let hash = 2_166_136_261;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 16_777_619);
  }
  return (hash >>> 0).toString(16);
}

export function cacheToken(fingerprint: string, revision: string): string {
  return `${fingerprint}:${revision}`;
}

/** A source counts as extracted when its file is unchanged since the last compile. */
export function sourceIsIngested(
  cached: string | null | undefined,
  fingerprint: string,
  revision: string,
): boolean {
  if (!fingerprint || !cached) return false;
  return cached === fingerprint || cached === cacheToken(fingerprint, revision);
}

function fillPrompt(
  template: string,
  vars: Record<string, string>,
  required: string[],
): string {
  let text = template;
  const missing: string[] = [];
  for (const key of Object.keys(vars)) {
    const token = `{{${key}}}`;
    if (text.includes(token)) text = text.split(token).join(vars[key] ?? "");
    else if (required.includes(key)) missing.push(key);
  }
  if (missing.length > 0) {
    text +=
      "\n\n" +
      missing.map((key) => `${key}:\n${vars[key] ?? ""}`).join("\n\n");
  }
  return text;
}

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
  instruction?: string;
}): string {
  const vars = {
    purpose: input.purpose || "(none)",
    schema: input.schema || "(none)",
    index: input.index || "(none)",
    sourcePath: input.sourcePath,
    source: input.source,
  };
  const custom = input.instruction?.trim() ?? "";
  const body = custom
    ? fillPrompt(custom, vars, ["purpose", "schema", "index", "sourcePath", "source"])
    : [
        "You analyze one source for a wiki. Reply with text only. Do not create or edit files.",
        "",
        "Purpose:",
        vars.purpose,
        "",
        "Schema:",
        vars.schema,
        "",
        "Index:",
        vars.index,
        "",
        `Source path: ${input.sourcePath}`,
        input.source,
      ].join("\n");
  return `${body}\n\n${LOCKED_ANALYSIS_RULES}`;
}

export function buildGenerationPrompt(input: {
  sourcePath: string;
  source: string;
  schema: string;
  analysis: string;
  instruction?: string;
}): string {
  const vars = {
    schema: input.schema || "(none)",
    analysis: input.analysis,
    sourcePath: input.sourcePath,
    source: input.source,
  };
  const custom = input.instruction?.trim() ?? "";
  const body = custom
    ? fillPrompt(custom, vars, ["schema", "analysis", "sourcePath", "source"])
    : [
        "Write wiki pages from the analysis. Reply with text only. Do not call tools. Do not create or edit files.",
        "The entire reply is one or more FILE blocks and nothing else. No introduction.",
        "Each page must use this exact fence:",
        "---FILE: wiki/concepts/example.md---",
        "page body",
        "---END FILE---",
        "Paths stay under wiki/ and end in .md.",
        "Do not emit wiki/index.md, wiki/overview.md, or wiki/log.md.",
        "Each page starts with frontmatter containing title, type, sources (the raw source path), and related.",
        "In the body, link an existing page as [[concepts/name]], using its path under wiki/ without the wiki/ prefix or .md.",
        "",
        "Schema:",
        vars.schema,
        "",
        "Analysis:",
        input.analysis,
        "",
        `Source path: ${input.sourcePath}`,
        input.source,
      ].join("\n");
  return `${body}\n\n${LOCKED_GENERATION_RULES}`;
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

async function readGuide(
  io: WikiIo,
  project: string,
  rootPath: string,
  wikiPath: string,
): Promise<string> {
  const root = await readOptional(io, project, rootPath);
  if (root.trim()) return root;
  return readOptional(io, project, wikiPath);
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
  prompts?: WikiPromptSet;
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
  prompts?: WikiPromptSet;
}): Promise<IngestResult> {
  args.onProgress?.("read");
  const fingerprint = await args.io.fingerprint(args.project, args.sourcePath);
  const token = cacheToken(fingerprint, promptRevision(args.prompts));
  const cached = await args.io.cacheGet(args.project, args.sourcePath);
  if (!args.force && cached === token) {
    return { skipped: true, written: [], usage: null, modelId: "" };
  }
  const [schema, purpose, index, source] = await Promise.all([
    readGuide(args.io, args.project, "schema.md", "wiki/schema.md"),
    readGuide(args.io, args.project, "purpose.md", "wiki/purpose.md"),
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
        instruction: args.prompts?.analyze,
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
        instruction: args.prompts?.generate,
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
  await args.io.cachePut(args.project, args.sourcePath, token);
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
