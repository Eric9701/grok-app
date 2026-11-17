/** Read-only knowledge graph. Edges come from page text, never from a second model call. */

export type WikiGraphPage = {
  path: string;
  content: string;
};

export type WikiGraphColor = "warning" | "info" | "accent" | "success" | "danger" | "muted";

export type WikiGraphNode = {
  id: string;
  path: string;
  label: string;
  type: string;
  folder: string;
  color: WikiGraphColor;
  linkCount: number;
};

export type WikiGraphEdge = {
  source: string;
  target: string;
  weight: number;
};

export type WikiGraph = {
  nodes: WikiGraphNode[];
  edges: WikiGraphEdge[];
};

const AGGREGATE = new Set(["wiki/index.md", "wiki/overview.md", "wiki/log.md"]);
const TYPE_AFFINITY: Record<string, Record<string, number>> = {
  entity: { concept: 1.2, entity: 0.8, source: 1.0, synthesis: 1.0, query: 0.8 },
  concept: { entity: 1.2, concept: 0.8, source: 1.0, synthesis: 1.2, query: 1.0 },
  source: { entity: 1.0, concept: 1.0, source: 0.5, query: 0.8, synthesis: 1.0 },
  query: { concept: 1.0, entity: 0.8, synthesis: 1.0, source: 0.8, query: 0.5 },
  synthesis: { concept: 1.2, entity: 1.0, source: 1.0, query: 1.0, synthesis: 0.8 },
};

type Draft = {
  id: string;
  path: string;
  label: string;
  type: string;
  folder: string;
  sources: string[];
  links: string[];
};

export function buildWikiGraph(pages: WikiGraphPage[]): WikiGraph {
  const drafts = new Map<string, Draft>();
  for (const page of pages) {
    const path = page.path.trim().replace(/\\/g, "/");
    if (!path.startsWith("wiki/") || !path.endsWith(".md") || AGGREGATE.has(path.toLowerCase())) continue;
    const frontmatter = frontmatterBlock(page.content);
    const type = scalar(frontmatter, "type").toLowerCase();
    if (type === "query") continue;
    const id = path.slice("wiki/".length, -".md".length);
    if (!id || drafts.has(id)) continue;
    drafts.set(id, {
      id,
      path,
      label: pageLabel(frontmatter, page.content, id),
      type: type || "other",
      folder: id.includes("/") ? id.slice(0, id.indexOf("/")) : "",
      sources: list(frontmatter, "sources"),
      links: Array.from(new Set([...wikilinkTargets(page.content), ...relatedStems(frontmatter)])),
    });
  }

  const stems = new Map<string, string[]>();
  for (const id of drafts.keys()) {
    const stem = id.split("/").pop()?.toLowerCase() ?? "";
    const listForStem = stems.get(stem) ?? [];
    listForStem.push(id);
    stems.set(stem, listForStem);
  }

  const outgoing = new Map<string, Set<string>>();
  const incoming = new Map<string, Set<string>>();
  for (const id of drafts.keys()) {
    outgoing.set(id, new Set());
    incoming.set(id, new Set());
  }
  for (const draft of drafts.values()) {
    for (const raw of draft.links) {
      const target = resolveTarget(raw, drafts, stems);
      if (!target || target === draft.id) continue;
      outgoing.get(draft.id)?.add(target);
      incoming.get(target)?.add(draft.id);
    }
  }

  const seen = new Set<string>();
  const edges: WikiGraphEdge[] = [];
  for (const [source, targets] of outgoing) {
    for (const target of targets) {
      const key = source < target ? `${source}\0${target}` : `${target}\0${source}`;
      if (seen.has(key)) continue;
      seen.add(key);
      edges.push({
        source: source < target ? source : target,
        target: source < target ? target : source,
        weight: relevance(source, target, drafts, outgoing, incoming),
      });
    }
  }
  edges.sort((a, b) => a.source.localeCompare(b.source) || a.target.localeCompare(b.target));

  const nodes: WikiGraphNode[] = Array.from(drafts.values())
    .map((draft) => {
      const neighbors = new Set([...(outgoing.get(draft.id) ?? []), ...(incoming.get(draft.id) ?? [])]);
      return {
        id: draft.id,
        path: draft.path,
        label: draft.label,
        type: draft.type,
        folder: draft.folder,
        color: colorFor(draft.folder),
        linkCount: neighbors.size,
      };
    })
    .sort((a, b) => a.id.localeCompare(b.id));

  return { nodes, edges };
}

function relevance(
  source: string,
  target: string,
  drafts: Map<string, Draft>,
  outgoing: Map<string, Set<string>>,
  incoming: Map<string, Set<string>>,
): number {
  const left = drafts.get(source);
  const right = drafts.get(target);
  if (!left || !right) return 1;
  const forward = outgoing.get(source)?.has(target) ? 1 : 0;
  const backward = outgoing.get(target)?.has(source) ? 1 : 0;
  const shared = right.sources.filter((item) => left.sources.includes(item)).length;
  const neighborsOf = (id: string) => {
    const found = new Set<string>(outgoing.get(id) ?? []);
    for (const item of incoming.get(id) ?? []) found.add(item);
    return found;
  };
  const degreeOf = (id: string) => (outgoing.get(id)?.size ?? 0) + (incoming.get(id)?.size ?? 0);
  let adamic = 0;
  const leftNeighbors = neighborsOf(source);
  for (const neighbor of neighborsOf(target)) {
    if (!leftNeighbors.has(neighbor)) continue;
    adamic += 1 / Math.log(Math.max(degreeOf(neighbor), 2));
  }
  const affinity = TYPE_AFFINITY[left.type]?.[right.type] ?? 0.5;
  return forward * 3 + backward * 3 + shared * 4 + adamic * 1.5 + affinity;
}

function resolveTarget(raw: string, drafts: Map<string, Draft>, stems: Map<string, string[]>): string | null {
  const target = raw.trim().replace(/\\/g, "/").replace(/\.md$/i, "").replace(/^wiki\//i, "");
  if (!target) return null;
  if (drafts.has(target)) return target;
  const folded = target.toLowerCase();
  for (const id of drafts.keys()) {
    if (id.toLowerCase() === folded) return id;
  }
  if (target.includes("/")) return null;
  const matches = stems.get(folded) ?? [];
  return matches.length === 1 ? matches[0] : null;
}

const FOLDER_COLOR: Record<string, WikiGraphColor> = {
  concepts: "accent",
  themes: "accent",
  thesis: "accent",
  entities: "success",
  characters: "success",
  stakeholders: "success",
  decisions: "warning",
  meetings: "warning",
  plot: "warning",
  projects: "danger",
  comparisons: "danger",
  habits: "danger",
  sources: "info",
  overview: "info",
  queries: "info",
  goals: "info",
  synthesis: "info",
  reflections: "info",
};

function colorFor(folder: string): WikiGraphColor {
  if (!folder) return "muted";
  const known = FOLDER_COLOR[folder];
  if (known) return known;
  const palette: WikiGraphColor[] = ["accent", "success", "warning", "danger", "info"];
  let hash = 0;
  for (const char of folder) hash = (hash * 33 + char.charCodeAt(0)) >>> 0;
  return palette[hash % palette.length];
}

function pageLabel(frontmatter: string, content: string, id: string): string {
  const title = scalar(frontmatter, "title");
  if (title) return title;
  const heading = content.match(/^#\s+(.+)$/m)?.[1]?.trim();
  if (heading) return heading;
  return id.split("/").pop() || id;
}

function wikilinkTargets(content: string): string[] {
  const links: string[] = [];
  for (const match of content.matchAll(/\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]/g)) {
    const target = match[1]?.trim().replace(/\\/g, "/").replace(/\.md$/i, "").replace(/^wiki\//i, "");
    if (target) links.push(target);
  }
  return links;
}

function relatedStems(frontmatter: string): string[] {
  return list(frontmatter, "related")
    .map((value) =>
      value
        .trim()
        .replace(/^\[\[|\]\]$/g, "")
        .split("|")[0]
        .split("#")[0]
        .replace(/\\/g, "/")
        .replace(/\.md$/i, "")
        .split("/")
        .pop()
        ?.trim() ?? "",
    )
    .filter(Boolean);
}

function frontmatterBlock(content: string): string {
  return content.match(/^---\r?\n([\s\S]*?)\r?\n---/)?.[1] ?? "";
}

function scalar(block: string, key: string): string {
  const match = block.match(new RegExp(`^${key}:\\s*(.+)$`, "im"));
  if (!match) return "";
  let value = match[1].trim();
  if (value.startsWith("[") || value.startsWith("|") || value.startsWith(">")) return "";
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    value = value.slice(1, -1).trim();
  }
  return value;
}

function list(block: string, key: string): string[] {
  const inline = block.match(new RegExp(`^${key}:\\s*\\[([^\\]]*)\\]`, "im"));
  if (inline) {
    return inline[1]
      .split(",")
      .map((item) => item.trim().replace(/^["']|["']$/g, ""))
      .filter(Boolean);
  }
  const lines = block.split(/\r?\n/);
  const start = lines.findIndex((line) => new RegExp(`^${key}:\\s*$`, "i").test(line));
  if (start < 0) return [];
  const items: string[] = [];
  for (const line of lines.slice(start + 1)) {
    const item = line.match(/^\s+-\s+(.*)$/);
    if (!item) break;
    const value = item[1].trim().replace(/^["']|["']$/g, "");
    if (value) items.push(value);
  }
  return items;
}
