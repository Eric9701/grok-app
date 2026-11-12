/** @vitest-environment jsdom */
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/test/jsdomStubs";
import { WikiLibrariesField } from "./WikiLibrariesField";

const { settingsGet, settingsSet, pickDirectory, wikiInit } = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(async (body: { wikiLibraries?: unknown }) => body),
  pickDirectory: vi.fn(),
  wikiInit: vi.fn(async () => "E:\\knowledge1\\test"),
}));

vi.mock("@/lib/api", () => ({
  isTauri: () => true,
  settingsGet: (...args: unknown[]) => settingsGet(...args),
  settingsSet: (...args: unknown[]) => settingsSet(...args),
  pickDirectory: (...args: unknown[]) => pickDirectory(...args),
  wikiInit: (...args: unknown[]) => wikiInit(...args),
}));

const t = (key: string, vars?: Record<string, string | number>) => {
  let text =
    ({
    "wiki.settingsTitle": "个人知识库",
    "wiki.settingsDesc": "说明",
    "wiki.addDirectory": "添加目录",
    "wiki.settingsEmpty": "还没有知识库目录。",
    "wiki.name": "名称",
    "wiki.agentDefault": "Agent 默认",
    "wiki.makeDefault": "设为 Agent 默认",
    "wiki.remove": "移除",
    "wiki.init": "初始化",
    "wiki.initTitle": "重新初始化",
    "wiki.initDesc": "重新选择类型。",
    "wiki.initNext": "继续",
    "wiki.initBack": "返回",
    "wiki.initConfirm": "将删除「{name}」里的全部页面和原文。",
    "wiki.initSubmit": "确认清除",
    "wiki.initDone": "已经清空并按所选类型初始化。",
    "wiki.initFailed": "这个目录没能初始化。",
    "wiki.template": "类型",
    "wiki.template.general": "通用",
    "wiki.template.research": "研究",
    "wiki.template.generalDesc": "最小的目录",
    "wiki.template.researchDesc": "深度研究",
    "wiki.template.reading": "阅读",
    "wiki.template.readingDesc": "阅读",
    "wiki.template.personal": "个人成长",
    "wiki.template.personalDesc": "成长",
    "wiki.template.business": "商务",
    "wiki.template.businessDesc": "商务",
    "common.cancel": "取消",
    "common.close": "关闭",
    "wiki.pathInvalid": "请选择这台电脑上的文件夹。",
    "wiki.duplicate": "这个目录已经在列表里。",
  })[key] ?? key;
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
};

afterEach(() => {
  cleanup();
  settingsGet.mockReset();
  settingsSet.mockReset();
  pickDirectory.mockReset();
  wikiInit.mockReset();
  settingsSet.mockImplementation(async (body) => body);
  wikiInit.mockImplementation(async () => "E:\\knowledge1\\test");
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

  it("shows a windows path without the verbatim prefix", async () => {
    settingsGet.mockResolvedValue({
      wikiLibraries: [
        {
          id: "a",
          name: "test",
          path: "\\\\?\\E:\\knowledge1\\test",
          agentDefault: true,
        },
      ],
    });
    render(<WikiLibrariesField t={t} rowHighlight={() => ""} />);
    expect(await screen.findByText("E:\\knowledge1\\test")).toBeTruthy();
    expect(screen.queryByText(/\\\\\?\\/)).toBeNull();
    await waitFor(() => {
      const saved = settingsSet.mock.calls.at(-1)?.[0] as {
        wikiLibraries: Array<{ path: string }>;
      };
      expect(saved.wikiLibraries[0]?.path).toBe("E:\\knowledge1\\test");
    });
  });

  it("reinitializes a library with the type chosen in the dialog", async () => {
    settingsGet.mockResolvedValue({
      wikiLibraries: [
        {
          id: "a",
          name: "test",
          path: "E:\\knowledge1\\test",
          agentDefault: true,
        },
      ],
    });
    render(<WikiLibrariesField t={t} rowHighlight={() => ""} />);
    fireEvent.click(await screen.findByRole("button", { name: "初始化" }));
    fireEvent.click(await screen.findByRole("radio", { name: /研究/ }));
    fireEvent.click(screen.getByRole("button", { name: "继续" }));
    expect(await screen.findByText(/将删除「test」里的全部页面和原文/)).toBeTruthy();
    expect(wikiInit).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认清除" }));
    await waitFor(() => {
      expect(wikiInit).toHaveBeenCalledWith("E:\\knowledge1\\test", "research");
    });
    expect(await screen.findByText("已经清空并按所选类型初始化。")).toBeTruthy();
  });
});
