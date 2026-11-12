/** Group compiled pages the way the knowledge rail shows them. */

const FOLDER_LABEL: Record<string, string> = {
  entities: "wiki.kind.entities",
  concepts: "wiki.kind.concepts",
  sources: "wiki.kind.sources",
  queries: "wiki.kind.queries",
  comparisons: "wiki.kind.comparisons",
  synthesis: "wiki.kind.synthesis",
  thesis: "wiki.kind.thesis",
  characters: "wiki.kind.characters",
  themes: "wiki.kind.themes",
  plot: "wiki.kind.plot",
  goals: "wiki.kind.goals",
  habits: "wiki.kind.habits",
  reflections: "wiki.kind.reflections",
  meetings: "wiki.kind.meetings",
  decisions: "wiki.kind.decisions",
  projects: "wiki.kind.projects",
  stakeholders: "wiki.kind.stakeholders",
};

const FOLDER_ORDER = Object.keys(FOLDER_LABEL);

export type WikiGroup = {
  key: string;
  labelKey: string;
  label: string;
  pages: string[];
};

export function groupWikiPages(pages: string[]): WikiGroup[] {
  const overview: string[] = [];
  const buckets = new Map<string, string[]>();
  for (const page of pages) {
    const rel = page.replace(/^wiki\//, "");
    const slash = rel.indexOf("/");
    if (slash < 0) {
      overview.push(page);
      continue;
    }
    const folder = rel.slice(0, slash);
    const list = buckets.get(folder) ?? [];
    list.push(page);
    buckets.set(folder, list);
  }
  const groups: WikiGroup[] = [];
  if (overview.length > 0) {
    groups.push({
      key: "overview",
      labelKey: "wiki.kind.overview",
      label: "",
      pages: overview,
    });
  }
  const seen = new Set<string>();
  for (const folder of FOLDER_ORDER) {
    const listed = buckets.get(folder);
    if (!listed?.length) continue;
    seen.add(folder);
    groups.push({
      key: folder,
      labelKey: FOLDER_LABEL[folder] ?? "",
      label: folder,
      pages: listed,
    });
  }
  for (const [folder, listed] of buckets) {
    if (seen.has(folder)) continue;
    groups.push({ key: folder, labelKey: "", label: folder, pages: listed });
  }
  return groups;
}

export type WikiTreeNode = {
  name: string;
  path: string;
  dir: boolean;
  children: WikiTreeNode[];
};

export function buildWikiTree(files: string[], dirs: string[]): WikiTreeNode[] {
  const root: WikiTreeNode = { name: "", path: "", dir: true, children: [] };
  const index = new Map<string, WikiTreeNode>([["", root]]);
  const ensure = (path: string, dir: boolean) => {
    const existing = index.get(path);
    if (existing) {
      if (dir) existing.dir = true;
      return existing;
    }
    const name = path.split("/").pop() || path;
    const node: WikiTreeNode = { name, path, dir, children: [] };
    index.set(path, node);
    const parentPath = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
    ensure(parentPath, true).children.push(node);
    return node;
  };
  for (const dir of dirs) {
    if (dir.trim()) ensure(dir, true);
  }
  for (const file of files) {
    if (file.trim()) ensure(file, false);
  }
  const sortNodes = (nodes: WikiTreeNode[]) => {
    nodes.sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name));
    for (const node of nodes) sortNodes(node.children);
  };
  sortNodes(root.children);
  return root.children;
}
