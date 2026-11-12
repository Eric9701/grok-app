import { describe, expect, it, vi } from "vitest";
import {
  buildAnalysisPrompt,
  fallbackWikiPage,
  ingest,
  pagePathOk,
  parseFileBlocks,
  type WikiIo,
  type WikiPage,
} from "./engine";

function io(partial: Partial<WikiIo> = {}): WikiIo {
  return {
    read: vi.fn(async (_project: string, path: string) => {
      if (path === "raw/sources/note.md") return "A pump moves water.";
      if (path.endsWith(".md")) return "";
      throw new Error("wiki_missing");
    }),
    writePages: vi.fn(async (_project: string, pages: WikiPage[]) =>
      pages.map((page) => page.path),
    ),
    fingerprint: vi.fn(async () => "abc"),
    cacheGet: vi.fn(async () => null),
    cachePut: vi.fn(async () => undefined),
    ...partial,
  };
}

describe("wiki engine", () => {
  it("removes the leftover source page after real pages are written", async () => {
    const bound = io();
    bound.remove = vi.fn(async () => undefined);
    await ingest({
      project: "/proj",
      sourcePath: "raw/sources/note.md",
      io: bound,
      complete: async (prompt) =>
        prompt.includes("Purpose:")
          ? "entities"
          : "---FILE: wiki/index.md---\n# Index\n---FILE: wiki/concepts/a.md---\n# A\n---END FILE---",
    });
    expect(bound.remove).toHaveBeenCalledWith("/proj", "wiki/sources/note.md");
  });

  it("does not save a CLI model rejection as a wiki page", () => {
    const text =
      "Couldn't set model 'arch-minimax': Invalid params: \"unknown model id\". Run 'grok models' to see available models.";
    expect(fallbackWikiPage("raw/sources/note.md", text)).toBeNull();
  });

  it("parses file blocks and rejects an escaped path before writing", async () => {
    expect(parseFileBlocks("---FILE: wiki/a.md---\nbody\n---END FILE---")).toEqual([
      { path: "wiki/a.md", content: "body\n" },
    ]);
    expect(
      parseFileBlocks(
        "---FILE: wiki/index.md---\n# Index\n---FILE: wiki/concepts/a.md---\n# A\n---END FILE---",
      ),
    ).toEqual([
      { path: "wiki/index.md", content: "# Index\n" },
      { path: "wiki/concepts/a.md", content: "# A\n" },
    ]);
    expect(
      parseFileBlocks("```md\n--- FILE: wiki/a.md ---\r\nbody\r\n--- END FILE ---\n```"),
    ).toEqual([{ path: "wiki/a.md", content: "body\n" }]);
    expect(pagePathOk("wiki/../x.md")).toBe(false);
    const bound = io();
    await expect(
      ingest({
        project: "/proj",
        sourcePath: "raw/sources/note.md",
        io: bound,
        complete: async (prompt) => {
          if (prompt.startsWith("You analyze")) return "analysis";
          return "---FILE: wiki/../x.md---\nno\n---END FILE---";
        },
      }),
    ).rejects.toThrow("wiki_path");
    expect(bound.writePages).not.toHaveBeenCalled();
  });

  it("skips a source whose fingerprint is unchanged", async () => {
    const complete = vi.fn(async () => "no");
    const bound = io({ cacheGet: vi.fn(async () => "abc") });
    const result = await ingest({
      project: "/proj",
      sourcePath: "raw/sources/note.md",
      io: bound,
      complete,
    });
    expect(result).toEqual({ skipped: true, written: [], usage: null, modelId: "" });
    expect(complete).not.toHaveBeenCalled();
  });

  it("compiles again when forced even if the fingerprint matches", async () => {
    const bound = io({ cacheGet: vi.fn(async () => "abc") });
    const result = await ingest({
      project: "/proj",
      sourcePath: "raw/sources/note.md",
      io: bound,
      force: true,
      complete: async (prompt) =>
        prompt.includes("Purpose:")
          ? "entities"
          : "---FILE: wiki/concepts/pump.md---\n# 水泵\n---END FILE---",
    });
    expect(result.skipped).toBe(false);
    expect(bound.writePages).toHaveBeenCalledWith(
      "/proj",
      [{ path: "wiki/concepts/pump.md", content: "# 水泵\n" }],
      true,
    );
  });

  it("writes pages from the second completion and remembers the fingerprint", async () => {
    const bound = io();
    const result = await ingest({
      project: "/proj",
      sourcePath: "raw/sources/note.md",
      io: bound,
      complete: async (prompt) => {
        if (prompt.includes("Purpose:")) return "entities: pump";
        return "---FILE: wiki/concepts/pump.md---\n# 水泵\n---END FILE---";
      },
    });
    expect(result.written).toEqual(["wiki/concepts/pump.md"]);
    const aggregate = vi.mocked(bound.writePages).mock.calls[1];
    expect(aggregate?.[0]).toBe("/proj");
    expect(aggregate?.[2]).toBe(true);
    const files = aggregate?.[1] as Array<{ path: string; content: string }>;
    expect(files.map((file) => file.path)).toEqual([
      "wiki/index.md",
      "wiki/overview.md",
      "wiki/log.md",
    ]);
    expect(files[0]?.content).toContain("- [[concepts/pump]] — 水泵");
    expect(files[1]?.content).toContain("<!-- llm-wiki:recent-start -->");
    expect(files[1]?.content).toContain("- [[concepts/pump]] — 水泵");
    expect(files[2]?.content).toContain("ingest | raw/sources/note.md");
    expect(result.usage).toBeNull();
    expect(bound.cachePut).toHaveBeenCalledWith("/proj", "raw/sources/note.md", "abc");
    expect(buildAnalysisPrompt({
      sourcePath: "raw/sources/note.md",
      source: "A pump",
      schema: "",
      purpose: "",
      index: "",
    })).toContain("Do not create or edit files.");
  });

  it("adds token counts from both model calls", async () => {
    const result = await ingest({
      project: "/proj",
      sourcePath: "raw/sources/note.md",
      io: io(),
      complete: async (prompt) => {
        if (prompt.includes("Purpose:")) {
          return { text: "entities", usage: { inputTokens: 10, outputTokens: 2, totalTokens: 12 }, modelId: "grok" };
        }
        return {
          text: "---FILE: wiki/concepts/pump.md---\n# 水泵\n---END FILE---",
          usage: { inputTokens: 4, outputTokens: 6, totalTokens: 10 },
          modelId: "grok",
        };
      },
    });
    expect(result.usage).toEqual({ inputTokens: 14, outputTokens: 8, totalTokens: 22 });
    expect(result.modelId).toBe("grok");
  });

  it("asks again and then keeps a prose reply as one source page", async () => {
    const calls: string[] = [];
    const prose =
      "这是一篇关于编译失败的说明。模型没有交回页面标记，正文仍然应该留下来，供知识库阅读。";
    const result = await ingest({
      project: "/proj",
      sourcePath: "raw/sources/Claude Code总记.md",
      io: io({
        read: vi.fn(async () => prose),
      }),
      complete: async (prompt) => {
        calls.push(prompt.slice(0, 24));
        if (prompt.includes("Purpose:")) return "analysis";
        if (prompt.startsWith("The previous reply")) return prose;
        return prose;
      },
    });
    expect(calls.some((item) => item.startsWith("The previous reply"))).toBe(true);
    expect(result.written).toEqual(["wiki/sources/Claude-Code总记.md"]);
  });

  it("reports read, analyze, generate, then write", async () => {
    const stages: string[] = [];
    const bound = io();
    await ingest({
      project: "/proj",
      sourcePath: "raw/sources/note.md",
      io: bound,
      onProgress: (stage) => stages.push(stage),
      complete: async (prompt) =>
        prompt.startsWith("You analyze")
          ? "entities"
          : "---FILE: wiki/concepts/pump.md---\n# Pump\n---END FILE---",
    });
    expect(stages).toEqual(["read", "analyze", "generate", "write"]);
  });

  it("runs one ingest at a time for the same project", async () => {
    const order: string[] = [];
    let releaseFirst: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    const bound = io();
    const first = ingest({
      project: "/same",
      sourcePath: "raw/sources/note.md",
      io: bound,
      complete: async (prompt) => {
        if (prompt.includes("Purpose:")) {
          order.push("first-analysis");
          await gate;
          return "one";
        }
        order.push("first-write");
        return "---FILE: wiki/a.md---\nA\n---END FILE---";
      },
    });
    const second = ingest({
      project: "/same",
      sourcePath: "raw/sources/note.md",
      io: bound,
      complete: async (prompt) => {
        if (prompt.includes("Purpose:")) {
          order.push("second-analysis");
          return "two";
        }
        return "---FILE: wiki/b.md---\nB\n---END FILE---";
      },
    });
    for (let i = 0; i < 20 && order.length === 0; i += 1) {
      await Promise.resolve();
    }
    expect(order).toEqual(["first-analysis"]);
    releaseFirst();
    await Promise.all([first, second]);
    expect(order).toEqual(["first-analysis", "first-write", "second-analysis"]);
  });
});
