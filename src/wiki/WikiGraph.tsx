import { useEffect, useMemo, useRef, useState } from "react";
import Graph from "graphology";
import forceAtlas2 from "graphology-layout-forceatlas2";
import Sigma from "sigma";
import type { MessageKey } from "@/i18n";
import { wikiReadPages, type WikiText } from "@/lib/api/wiki";
import { buildWikiGraph, type WikiGraphColor } from "./graph";

const LAYOUT_CAP = 400;
const LAYOUT_VERSION = "spread2";
const layoutMemory = new Map<string, Map<string, { x: number; y: number }>>();

const KIND_KEY: Record<string, MessageKey> = {
  overview: "wiki.kind.overview",
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

export function WikiGraph({
  projectPath,
  pages,
  tr,
  onOpen,
}: {
  projectPath: string;
  pages: string[];
  tr: (key: MessageKey) => string;
  onOpen: (path: string) => void;
}) {
  const [rows, setRows] = useState<WikiText[] | null>(null);
  const [failed, setFailed] = useState(false);
  const canvasRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<Sigma | null>(null);
  const onOpenRef = useRef(onOpen);
  onOpenRef.current = onOpen;
  const wikiPages = useMemo(
    () => pages.filter((path) => path.startsWith("wiki/") && path.endsWith(".md")).slice(0, LAYOUT_CAP),
    [pages],
  );
  const model = useMemo(() => buildWikiGraph(rows ?? []), [rows]);

  useEffect(() => {
    let stop = false;
    setRows(null);
    setFailed(false);
    if (!projectPath) {
      setRows([]);
      return;
    }
    wikiReadPages(projectPath, wikiPages)
      .then((loaded) => {
        if (!stop) setRows(loaded);
      })
      .catch(() => {
        if (!stop) setFailed(true);
      });
    return () => {
      stop = true;
    };
  }, [projectPath, wikiPages]);

  useEffect(() => {
    const host = canvasRef.current;
    if (!host || failed || model.nodes.length === 0) return;
    const graph = new Graph();
    const colors = readGraphColors();
    for (const node of model.nodes) {
      graph.addNode(node.id, {
        x: 0,
        y: 0,
        size: 4 + Math.sqrt(node.linkCount) * 1.4,
        label: node.label,
        color: colors[node.color],
        pagePath: node.path,
      });
    }
    for (const edge of model.edges) {
      if (!graph.hasNode(edge.source) || !graph.hasNode(edge.target)) continue;
      graph.addEdge(edge.source, edge.target, {
        size: Math.min(4, 0.5 + edge.weight / 4),
        color: colors.muted,
      });
    }
    const key = `${LAYOUT_VERSION}\0${projectPath}\0${model.nodes.map((node) => node.id).sort().join("\0")}`;
    placeGraph(graph, key);
    let focus: string | null = null;
    const linked = (node: string) =>
      focus !== null && (node === focus || graph.hasEdge(focus, node) || graph.hasEdge(node, focus));
    let renderer: Sigma | null = null;
    try {
      renderer = new Sigma(graph, host, {
        settings: { allowInvalidContainer: true },
        nodeReducer: (node, data) => {
          if (!focus) return {};
          if (linked(node)) {
            return {
              opacity: 1,
              labelVisibility: "visible",
              zIndex: node === focus ? 2 : 1,
              size: data.size * (node === focus ? 1.55 : 1.2),
            };
          }
          return { opacity: 0.14, labelVisibility: "hidden", zIndex: 0 };
        },
        edgeReducer: (edge, data) => {
          if (!focus) return {};
          const source = graph.source(edge);
          const target = graph.target(edge);
          if (source === focus || target === focus) {
            const color = graph.getNodeAttribute(focus, "color");
            return {
              opacity: 1,
              size: Math.max(data.size, 2),
              zIndex: 1,
              color: typeof color === "string" ? color : data.color,
            };
          }
          return { opacity: 0.05, zIndex: 0 };
        },
      });
      const show = (node: string) => {
        focus = node;
        renderer?.refresh();
      };
      const clear = () => {
        if (!focus) return;
        focus = null;
        renderer?.refresh();
      };
      renderer.on("enterNode", ({ node }) => show(node));
      renderer.on("enterNodeLabel", ({ node }) => show(node));
      renderer.on("leaveNode", clear);
      renderer.on("leaveNodeLabel", clear);
      renderer.on("clickNode", ({ node }) => {
        const path = graph.getNodeAttribute(node, "pagePath");
        if (typeof path === "string") onOpenRef.current(path);
      });
      renderer.on("webglContextLost", () => setFailed(true));
      rendererRef.current = renderer;
    } catch {
      setFailed(true);
    }
    return () => {
      renderer?.kill();
      if (rendererRef.current === renderer) rendererRef.current = null;
    };
  }, [failed, model, projectPath]);

  const colors = readGraphColors();
  const legend = Array.from(
    new Map(model.nodes.map((node) => [node.folder, node.color])).entries(),
  )
    .map(([folder, color]) => {
      const key = KIND_KEY[folder];
      return {
        folder,
        color: colors[color],
        label: folder ? (key ? tr(key) : folder) : tr("wiki.graphOther"),
      };
    })
    .sort((a, b) => a.label.localeCompare(b.label));

  const zoom = (direction: "in" | "out" | "fit") => {
    const camera = rendererRef.current?.getCamera();
    if (!camera) return;
    if (direction === "in") void camera.zoomIn();
    else if (direction === "out") void camera.zoomOut();
    else void camera.reset();
  };

  return (
    <div className="wiki-graph">
      {rows === null && !failed ? <p className="wiki-graph__note">{tr("wiki.loading")}</p> : null}
      {failed ? <p className="wiki-graph__note">{tr("wiki.graphFailed")}</p> : null}
      {!failed && rows !== null && model.nodes.length === 0 ? (
        <p className="wiki-graph__note">{tr("wiki.graphEmpty")}</p>
      ) : null}
      {!failed && model.nodes.length > 0 ? (
        <div className="wiki-graph__tools">
          <button type="button" className="btn btn--sm btn--ghost" onClick={() => zoom("in")}>
            +
          </button>
          <button type="button" className="btn btn--sm btn--ghost" onClick={() => zoom("out")}>
            −
          </button>
          <button type="button" className="btn btn--sm btn--ghost" onClick={() => zoom("fit")}>
            {tr("wiki.graphFit")}
          </button>
        </div>
      ) : null}
      {!failed && model.edges.length === 0 && model.nodes.length > 0 ? (
        <p className="wiki-graph__note">{tr("wiki.graphEmpty")}</p>
      ) : null}
      {!failed && legend.length > 0 ? (
        <ul className="wiki-graph__legend" aria-label={tr("wiki.graphLegend")}>
          {legend.map((item) => (
            <li key={item.folder || "root"}>
              <span className="wiki-graph__swatch" style={{ background: item.color }} />
              <span>{item.label}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <div ref={canvasRef} className="wiki-graph__canvas" />
    </div>
  );
}

function placeGraph(graph: Graph, key: string) {
  const saved = layoutMemory.get(key);
  const nodes = graph.nodes();
  const count = nodes.length;
  nodes.forEach((id, index) => {
    const cached = saved?.get(id);
    if (cached) {
      graph.mergeNodeAttributes(id, cached);
      return;
    }
    const angle = (Math.PI * 2 * index) / Math.max(count, 1);
    const radius = count <= 1 ? 0 : Math.max(90, count * 10);
    graph.mergeNodeAttributes(id, { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius });
  });
  const complete = saved != null && nodes.every((id) => saved.has(id));
  if (!complete && count > 1 && count <= LAYOUT_CAP) {
    try {
      forceAtlas2.assign(graph, {
        iterations: count > 120 ? 80 : 140,
        settings: {
          gravity: 0.04,
          scalingRatio: 48,
          slowDown: 6,
          adjustSizes: true,
          outboundAttractionDistribution: true,
          edgeWeightInfluence: 0,
          barnesHutOptimize: count > 80,
        },
      });
    } catch {
      // Circle positions remain when the layout cannot run.
    }
  }
  const next = new Map<string, { x: number; y: number }>();
  graph.forEachNode((id, attributes) => {
    const x = attributes.x;
    const y = attributes.y;
    if (typeof x === "number" && typeof y === "number") next.set(id, { x, y });
  });
  layoutMemory.set(key, next);
}

function readGraphColors(): Record<WikiGraphColor, string> {
  const style = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    warning: read("--warning", "#c9a227"),
    info: read("--info", "#3d8bfd"),
    accent: read("--accent", "#d2a8ff"),
    success: read("--success", "#3fb950"),
    danger: read("--danger", "#f07178"),
    muted: read("--text-tertiary", "#8b949e"),
  };
}
