/** @vitest-environment jsdom */
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const { listeners } = vi.hoisted(() => ({
  listeners: new Map<string, Array<(payload: string) => void>>(),
}));
import "@/test/jsdomStubs";
import { resetWikiCompileTasksForTests } from "./compileTasks";
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
  wikiListDirs: vi.fn(async () => ["raw", "raw/sources", "wiki", "wiki/concepts"]),
  wikiImportFiles: vi.fn(async () => ["raw/sources/note.md"]),
  wikiImportUrl: vi.fn(async () => "raw/sources/page.md"),
  wikiRemoveRaw: vi.fn(async () => undefined),
  wikiScaffold: vi.fn(async () => "E:/notes"),
  pickAttachFiles: vi.fn(async () => []),
  pickDirectory: vi.fn(async () => null),
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
  listen: vi.fn(async (event: string, handler: (payload: string) => void) => {
    const list = listeners.get(event) ?? [];
    list.push(handler);
    listeners.set(event, list);
    return () => {};
  }),
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

afterEach(() => {
  cleanup();
  resetWikiCompileTasksForTests();
});

describe("WikiPage", () => {
  it("lists a page and opens it from the single knowledge rail", async () => {
    render(<WikiPage locale="zh" />);
    const page = await screen.findByRole("button", { name: "concepts/pump" });
    fireEvent.click(page);
    expect(await screen.findByText("水泵")).toBeTruthy();
    expect(screen.getByText(/缺少 missing/)).toBeTruthy();
    expect(screen.queryByText(/20 token/)).toBeNull();
    expect(screen.queryByText("Grok 4.7")).toBeNull();
  });

  it("says the example source is missing instead of claiming there are no pages", async () => {
    vi.mocked(ingestWithAtlas).mockRejectedValueOnce("wiki_missing");
    render(<WikiPage locale="zh" />);
    fireEvent.click(await screen.findByRole("button", { name: "原始资料" }));
    expect(await screen.findByText("sources/note.md")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "提取" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "找不到这篇原文：raw/sources/note.md",
    );
    expect(screen.queryByText("这个项目还没有知识库页面。")).toBeNull();
  });

  it("asks before compiling a source that is already recorded", async () => {
    const { wikiFingerprint, wikiCacheGet } = await import("@/lib/api");
    vi.mocked(wikiFingerprint).mockResolvedValue("abc");
    vi.mocked(wikiCacheGet).mockResolvedValue("abc");
    try {
      render(<WikiPage locale="zh" />);
      fireEvent.click(await screen.findByRole("button", { name: "原始资料" }));
      expect(await screen.findByText("sources/note.md")).toBeTruthy();
      vi.mocked(ingestWithAtlas).mockClear();
      fireEvent.click(screen.getByRole("button", { name: "提取" }));
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
    } finally {
      vi.mocked(wikiFingerprint).mockResolvedValue("new");
      vi.mocked(wikiCacheGet).mockResolvedValue(null);
    }
  });

  it("says compilation has started after the Atlas CLI starts", async () => {
    vi.mocked(ingestWithAtlas).mockImplementationOnce(() => new Promise(() => {}));
    try {
      const toast = vi.fn();
      render(<WikiPage locale="zh" onToast={toast} />);
      fireEvent.click(await screen.findByRole("button", { name: "原始资料" }));
      expect(await screen.findByText("sources/note.md")).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "提取" }));
      expect(await screen.findByRole("button", { name: "正在编译…" })).toBeTruthy();
      expect(await screen.findByRole("status")).toHaveTextContent(
        /正在编译 · 读取原文 · \d+:\d{2}/,
      );
      const notify = listeners.get("wiki://cli-started")?.at(-1);
      expect(notify).toBeTypeOf("function");
      notify!("E:/notes");
      expect(screen.getByRole("status")).toHaveTextContent(/正在编译 · 读取原文 · \d+:\d{2}/);
      expect(toast).toHaveBeenCalledWith("已经开始编译。");
      notify!("E:/notes");
      expect(toast).toHaveBeenCalledTimes(1);
      notify!("E:/other");
      expect(toast).toHaveBeenCalledTimes(1);
    } finally {
      vi.mocked(ingestWithAtlas).mockReset();
      vi.mocked(ingestWithAtlas).mockImplementation(async () => ({
        skipped: true,
        written: [],
      }));
    }
  });

  it("asks for a configured library when settings are empty", async () => {
    const { settingsGet } = await import("@/lib/api");
    vi.mocked(settingsGet).mockResolvedValueOnce({ wikiLibraries: [] } as never);
    render(<WikiPage locale="zh" />);
    expect(await screen.findByText("先在设置里添加一个知识库目录。")).toBeTruthy();
  });
});
