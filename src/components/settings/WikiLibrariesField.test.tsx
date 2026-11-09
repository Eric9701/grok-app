/** @vitest-environment jsdom */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/test/jsdomStubs";
import { WikiLibrariesField } from "./WikiLibrariesField";

const { settingsGet, settingsSet, pickDirectory } = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(async (body: { wikiLibraries?: unknown }) => body),
  pickDirectory: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  isTauri: () => true,
  settingsGet: (...args: unknown[]) => settingsGet(...args),
  settingsSet: (...args: unknown[]) => settingsSet(...args),
  pickDirectory: (...args: unknown[]) => pickDirectory(...args),
}));

const t = (key: string) =>
  ({
    "wiki.settingsTitle": "个人知识库",
    "wiki.settingsDesc": "说明",
    "wiki.addDirectory": "添加目录",
    "wiki.settingsEmpty": "还没有知识库目录。",
    "wiki.name": "名称",
    "wiki.agentDefault": "Agent 默认",
    "wiki.makeDefault": "设为 Agent 默认",
    "wiki.remove": "移除",
    "wiki.pathInvalid": "请选择这台电脑上的文件夹。",
    "wiki.duplicate": "这个目录已经在列表里。",
  })[key] ?? key;

afterEach(() => {
  cleanup();
  settingsGet.mockReset();
  settingsSet.mockReset();
  pickDirectory.mockReset();
  settingsSet.mockImplementation(async (body) => body);
});

describe("WikiLibrariesField", () => {
  it("shows the empty state when settings have no library", async () => {
    settingsGet.mockResolvedValue({ wikiLibraries: [] });
    render(<WikiLibrariesField t={t} rowHighlight={() => ""} />);
    expect(await screen.findByText("还没有知识库目录。")).toBeTruthy();
  });

  it("adds a folder and marks the first one as the agent default", async () => {
    settingsGet.mockResolvedValue({ wikiLibraries: [] });
    pickDirectory.mockResolvedValue("E:/notes");
    render(<WikiLibrariesField t={t} rowHighlight={() => ""} />);
    await screen.findByText("还没有知识库目录。");
    fireEvent.click(screen.getByRole("button", { name: "添加目录" }));
    expect(await screen.findByDisplayValue("notes")).toBeTruthy();
    const saved = settingsSet.mock.calls[0]?.[0] as {
      wikiLibraries: Array<{ path: string; agentDefault?: boolean }>;
    };
    expect(saved.wikiLibraries[0]?.path).toBe("E:/notes");
    expect(saved.wikiLibraries[0]?.agentDefault).toBe(true);
  });
});
