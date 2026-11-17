import { describe, expect, it } from "vitest";
import { buildWikiGraph } from "./graph";

function page(path: string, content: string) {
  return { path, content };
}

describe("wiki graph", () => {
  it("links a relative path and a unique short name", () => {
    const graph = buildWikiGraph([
      page(
        "wiki/concepts/pump.md",
        ["---", "title: 水泵", "type: concept", "---", "", "See [[concepts/valve]] and [[pipe]]."].join("\n"),
      ),
      page("wiki/concepts/valve.md", "# 阀门\n"),
      page("wiki/entities/pipe.md", "# 管道\n"),
    ]);
    expect(graph.nodes.map((node) => node.id)).toEqual([
      "concepts/pump",
      "concepts/valve",
      "entities/pipe",
    ]);
    expect(graph.edges).toEqual([
      expect.objectContaining({ source: "concepts/pump", target: "concepts/valve" }),
      expect.objectContaining({ source: "concepts/pump", target: "entities/pipe" }),
    ]);
  });

  it("does not connect a short name that belongs to two pages", () => {
    const graph = buildWikiGraph([
      page("wiki/concepts/pump.md", "[[pump]]\n"),
      page("wiki/entities/pump.md", "# Pump\n"),
    ]);
    expect(graph.edges).toEqual([]);
  });

  it("drops aggregate pages, query pages, and links that point at them", () => {
    const graph = buildWikiGraph([
      page("wiki/index.md", "[[concepts/pump]]\n"),
      page("wiki/overview.md", "[[concepts/pump]]\n"),
      page("wiki/log.md", "[[concepts/pump]]\n"),
      page("wiki/queries/saved.md", "---\ntype: query\n---\n\n[[concepts/pump]]\n"),
      page("wiki/concepts/pump.md", "See [[index]] and [[queries/saved]].\n"),
    ]);
    expect(graph.nodes.map((node) => node.id)).toEqual(["concepts/pump"]);
    expect(graph.edges).toEqual([]);
  });

  it("keeps one undirected edge when both pages link each other", () => {
    const graph = buildWikiGraph([
      page("wiki/concepts/pump.md", "[[concepts/valve]]\n"),
      page("wiki/concepts/valve.md", "[[concepts/pump]]\n"),
    ]);
    expect(graph.edges).toHaveLength(1);
    expect(graph.edges[0]?.weight).toBeGreaterThan(3);
  });

  it("does not connect pages that only share a source", () => {
    const graph = buildWikiGraph([
      page(
        "wiki/concepts/pump.md",
        "---\ntype: concept\nsources: [raw/sources/note.md]\nrelated: [\"[[concepts/valve]]\"]\n---\n\n# 水泵\n",
      ),
      page(
        "wiki/concepts/valve.md",
        "---\ntype: concept\nsources: [raw/sources/note.md]\n---\n\n# 阀门\n",
      ),
      page(
        "wiki/concepts/pipe.md",
        "---\ntype: concept\nsources: [raw/sources/note.md]\n---\n\n# 管道\n",
      ),
    ]);
    expect(graph.edges.map((edge) => [edge.source, edge.target])).toEqual([
      ["concepts/pump", "concepts/valve"],
    ]);
    const linked = buildWikiGraph([
      page("wiki/concepts/pump.md", "---\ntype: concept\n---\n\n[[concepts/valve]]\n"),
      page("wiki/concepts/valve.md", "---\ntype: concept\n---\n\n# 阀门\n"),
    ]);
    expect(graph.edges[0]?.weight).toBeGreaterThan(linked.edges[0]?.weight ?? 0);
  });

  it("paints each folder with a different color", () => {
    const graph = buildWikiGraph([
      page("wiki/concepts/a.md", "---\ntype: concept\n---\n"),
      page("wiki/decisions/b.md", "---\ntype: decision\n---\n"),
      page("wiki/entities/c.md", "---\ntype: entity\n---\n"),
      page("wiki/projects/d.md", "---\ntype: project\n---\n"),
      page("wiki/note.md", "---\ntype: note\n---\n"),
    ]);
    const colors = graph.nodes.map((node) => node.color);
    expect(new Set(colors).size).toBe(5);
    expect(colors).toEqual(["accent", "warning", "success", "muted", "danger"]);
  });
});
