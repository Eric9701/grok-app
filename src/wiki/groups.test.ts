import { describe, expect, it } from "vitest";
import { buildWikiTree, groupWikiPages } from "./groups";

describe("wiki groups", () => {
  it("puts root pages under overview and folders under their kind", () => {
    const groups = groupWikiPages([
      "wiki/index.md",
      "wiki/entities/ada.md",
      "wiki/concepts/pump.md",
    ]);
    expect(groups.map((group) => group.key)).toEqual(["overview", "entities", "concepts"]);
    expect(groups[1]?.pages).toEqual(["wiki/entities/ada.md"]);
  });

  it("builds a file tree that keeps empty directories", () => {
    const tree = buildWikiTree(
      ["raw/sources/note.md", "wiki/index.md"],
      ["raw", "raw/assets", "raw/sources", "wiki"],
    );
    expect(tree.map((node) => node.name)).toEqual(["raw", "wiki"]);
    const raw = tree[0];
    expect(raw?.children.map((node) => node.name)).toEqual(["assets", "sources"]);
    expect(raw?.children[1]?.children[0]?.path).toBe("raw/sources/note.md");
  });
});
