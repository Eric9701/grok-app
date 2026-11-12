/** Maintain index, overview, and log the way llm-wiki does after a compile.
 * The model does not rewrite these files. New pages are listed, and the log gains one line.
 */

const AGGREGATE = new Set(["wiki/index.md", "wiki/overview.md", "wiki/log.md"]);
const OVERVIEW_RECENT_START = "<!-- llm-wiki:recent-start -->";
const OVERVIEW_RECENT_END = "<!-- llm-wiki:recent-end -->";
const RECENT_HEADING = "## Recently Updated";

export function isAggregateWikiPath(path: string): boolean {
  return AGGREGATE.has(path.trim().replace(/\\/g, "/").toLowerCase());
}

export function wikiDate(now = new Date()): string {
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function buildIngestLog(existing: string, sourcePath: string, date: string): string {
  const entry = `## [${date}] ingest | ${sourcePath}`;
  return existing.trim()
    ? `${existing.trimEnd()}\n\n${entry}\n`
    : `# Wiki Log\n\n${entry}\n`;
}

export function updateRecentIndex(index: string, additions: string[]): string {
  const lines = index.trimEnd().split("\n");
  const start = lines.findIndex((line) => line.trim() === RECENT_HEADING);
  const prefix = start >= 0 ? lines.slice(0, start) : lines;
  const sectionEnd =
    start >= 0
      ? lines.findIndex((line, position) => position > start && /^##\s+/.test(line))
      : -1;
  const existing =
    start >= 0
      ? lines
          .slice(start + 1, sectionEnd >= 0 ? sectionEnd : undefined)
          .filter((line) => /^-\s+/.test(line))
      : [];
  const suffix = sectionEnd >= 0 ? lines.slice(sectionEnd) : [];
  const recent = Array.from(new Set([...additions, ...existing])).slice(0, 200);
  return [...prefix, "", RECENT_HEADING, ...recent, ...(suffix.length ? ["", ...suffix] : []), ""].join(
    "\n",
  );
}

export function updateRecentOverview(overview: string, additions: string[]): string {
  const base = overview.trim()
    ? overview
    : ["---", "type: overview", "title: Project Overview", "tags: []", "related: []", "---", "", "# Overview", ""].join(
        "\n",
      );
  const start = base.indexOf(OVERVIEW_RECENT_START);
  const end = base.indexOf(OVERVIEW_RECENT_END);
  const existing =
    start >= 0 && end > start
      ? base
          .slice(start + OVERVIEW_RECENT_START.length, end)
          .split("\n")
          .filter((line) => /^-\s+/.test(line))
      : [];
  const recent = Array.from(new Set([...additions, ...existing])).slice(0, 50);
  const block = [OVERVIEW_RECENT_START, RECENT_HEADING, ...recent, OVERVIEW_RECENT_END].join("\n");
  if (start >= 0 && end > start) {
    return `${base.slice(0, start)}${block}${base.slice(end + OVERVIEW_RECENT_END.length)}`;
  }
  return `${base.trimEnd()}\n\n${block}\n`;
}

function wikiTarget(path: string): string {
  return path.trim().replace(/\\/g, "/").replace(/^wiki\//i, "").replace(/\.md$/i, "");
}

function normalizeTarget(target: string): string {
  return wikiTarget(target).toLowerCase();
}

function linkedTargets(index: string): Set<string> {
  const found = new Set<string>();
  for (const match of index.matchAll(/\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]/g)) {
    const target = match[1]?.trim();
    if (target) found.add(normalizeTarget(target));
  }
  return found;
}

export function pageTitle(path: string, content: string): string {
  const fm = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (fm) {
    const line = fm[1].split(/\r?\n/).find((row) => /^title\s*:/i.test(row));
    if (line) {
      let value = line.replace(/^title\s*:\s*/i, "").trim();
      if (
        (value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'"))
      ) {
        value = value.slice(1, -1).trim();
      }
      if (value) return value;
    }
  }
  const heading = content.match(/^#\s+(.+)$/m);
  if (heading?.[1]?.trim()) return heading[1].trim();
  const name = path.split("/").pop() || path;
  return name.replace(/\.md$/i, "");
}

export function recentPageLines(
  index: string,
  pages: Array<{ path: string; content: string }>,
): string[] {
  const known = linkedTargets(index);
  const lines: string[] = [];
  for (const page of pages) {
    const path = page.path.trim().replace(/\\/g, "/");
    if (!path.startsWith("wiki/") || !path.endsWith(".md") || isAggregateWikiPath(path)) continue;
    const target = wikiTarget(path);
    const key = normalizeTarget(target);
    if (!target || known.has(key)) continue;
    known.add(key);
    lines.push(`- [[${target}]] — ${pageTitle(path, page.content)}`);
  }
  return lines;
}
