/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/test/jsdomStubs";
import { WikiPage } from "./WikiPage";
import { ingestWithAtlas } from "./host";

vi.mock("@/lib/api", () => ({
  isTauri: () => true,
  settingsGet: vi.fn(async () => ({
    wikiLibraries: [
      { id: "k", name: "笔记", path: "E:/notes", agentDefault: true },
    ],
  })),
  wikiList: vi.fn(async () => ["wiki/concepts/pump.md"]),
  wikiListRaw: vi.fn(async () => ["raw/sources/note.md"]),
  wikiFingerprint: vi.fn(async () => "new"),
  wikiCacheGet: vi.fn(async () => null),
  wikiLint: vi.fn(async () => ({
    broken: [{ path: "wiki/index.md", target: "missing" }],
    orphans: [],
  })),
  wikiRead: vi.fn(async () => ({
    path: "wiki/concepts/pump.md",
    content: "# 水泵",
  })),
  wikiSearch: vi.fn(async () => []),
  modelsListAvailable: vi.fn(async () => ({
    models: [{ id: "grok-4.7", label: "Grok 4.7", source: "official" }],
    defaultModelId: "grok-4.7",
  })),
  wikiUsageList: vi.fn(async () => [
    {
      at: "2026-11-09T02:00:00Z",
      source: "raw/sources/note.md",
      modelId: "grok",
      inputTokens: 12,
      outputTokens: 8,
      totalTokens: 20,
      reported: true,
    },
  ]),
}));

vi.mock("./host", () => ({
  ingestWithAtlas: vi.fn(async () => ({ skipped: true, written: [] })),
}));

afterEach(() => cleanup());

describe("WikiPage", () => {
  it("lists a page and opens it from the single knowledge rail", async () => {
    render(<WikiPage locale="zh" />);
    const page = await screen.findByRole("button", { name: "concepts/pump" });
    fireEvent.click(page);
    expect(await screen.findByText("水泵")).toBeTruthy();
    expect(screen.getByText(/缺少 missing/)).toBeTruthy();
    expect(screen.getAllByText(/20 token/).length).toBeGreaterThan(0);
    expect(await screen.findByText("Grok 4.7")).toBeTruthy();
  });

  it("says the example source is missing instead of claiming there are no pages", async () => {
    vi.mocked(ingestWithAtlas).mockRejectedValueOnce("wiki_missing");
    render(<WikiPage locale="zh" />);
    expect(await screen.findByText("sources/note.md")).toBeTruthy();
    fireEvent.submit(screen.getByRole("button", { name: "编译" }).closest("form")!);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "找不到这篇原文：raw/sources/note.md",
    );
    expect(screen.queryByText("这个项目还没有知识库页面。")).toBeNull();
  });

  it("asks before compiling a source that is already recorded", async () => {
    const { wikiFingerprint, wikiCacheGet } = await import("@/lib/api");
    vi.mocked(wikiFingerprint).mockResolvedValueOnce("abc");
    vi.mocked(wikiCacheGet).mockResolvedValueOnce("abc");
    render(<WikiPage locale="zh" />);
    expect(await screen.findByText("sources/note.md")).toBeTruthy();
    expect(await screen.findByText("Grok 4.7")).toBeTruthy();
    vi.mocked(ingestWithAtlas).mockClear();
    fireEvent.submit(screen.getByRole("button", { name: "编译" }).closest("form")!);
    expect(await screen.findByRole("dialog")).toHaveTextContent("已经编译过");
    expect(ingestWithAtlas).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "再次编译" }));
    await waitFor(() => {
      expect(ingestWithAtlas).toHaveBeenCalledWith(
        "E:/notes",
        "raw/sources/note.md",
        "grok-4.7",
        expect.any(Function),
        true,
      );
    });
  });

  it("asks for a configured library when settings are empty", async () => {
    const { settingsGet } = await import("@/lib/api");
    vi.mocked(settingsGet).mockResolvedValueOnce({ wikiLibraries: [] } as never);
    render(<WikiPage locale="zh" />);
    expect(await screen.findByText("先在设置里添加一个知识库目录。")).toBeTruthy();
  });
});
