/**
 * @vitest-environment jsdom
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SshRemoteSessionRail } from "./SshRemoteSessionRail";
import type { MessageKey, Vars } from "@/i18n";
import * as api from "@/lib/api";

const loadMore = vi.fn();
const onOpenSession = vi.fn();
const setDraftRemote = vi.fn();
const renameRemoteSession = vi.fn();

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    sshOpenSession: vi.fn(),
  };
});

vi.mock("@/providers/SshWatchProvider", () => ({
  useSshWatch: () => ({
    watchAliases: ["UTS"],
    sessionsByAlias: {
      UTS: [
        {
          id: "01a01907-adf3-7e00-a7a8-aee1082b0556",
          cwd: "/data/pengqlu/code/qwen35-v001-light",
          title: "01a01907-adf3-7e00-a7a8-aee1082b0556",
        },
        {
          id: "01a0192c-d7f4-7850-9e0c-3a342201cef10",
          cwd: "/data/pengqlu/code/idea",
          title: "帮我看一下 hallucination span",
        },
      ],
    },
    totalsByAlias: { UTS: 35 },
    titleOverlay: {},
    draftRemote: null,
    setDraftRemote,
    enableWatch: vi.fn(),
    disableWatch: vi.fn(),
    refreshSessions: vi.fn(),
    loadMore,
    renameRemoteSession,
  }),
}));

function t(k: MessageKey, vars?: Vars): string {
  if (k === "sidebar.remoteHost") return `远程 ${vars?.alias ?? ""}`;
  if (k === "sidebar.remoteUntitled") return "未命名";
  if (k === "sidebar.remoteRemaining") return `还有 ${vars?.n} 个`;
  if (k === "sidebar.remoteLoadMore") return "加载更多";
  if (k === "sidebar.remoteOpening") return "打开中…";
  if (k === "sidebar.remoteOpenFailed") return `无法打开：${vars?.error ?? ""}`;
  return String(k);
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SshRemoteSessionRail", () => {
  it("labels the host, hides the raw UUID, and offers load more", async () => {
    render(<SshRemoteSessionRail t={t} onOpenSession={onOpenSession} />);
    expect(screen.getByText("远程 UTS")).toBeInTheDocument();
    expect(screen.getByText("qwen35-v001-light")).toBeInTheDocument();
    expect(screen.getByText("帮我看一下 hallucination span")).toBeInTheDocument();
    expect(
      screen.queryByText("01a01907-adf3-7e00-a7a8-aee1082b0556"),
    ).toBeNull();
    expect(screen.getByText("还有 33 个")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "加载更多" }));
    expect(loadMore).toHaveBeenCalledWith("UTS");
  });

  it("shows opening on the clicked row", async () => {
    let resolveOpen: (v: api.SshOpenSessionResult) => void = () => {};
    vi.mocked(api.sshOpenSession).mockReturnValue(
      new Promise((r) => {
        resolveOpen = r;
      }),
    );
    render(<SshRemoteSessionRail t={t} onOpenSession={onOpenSession} />);
    await userEvent.click(
      screen.getByRole("button", { name: "qwen35-v001-light" }),
    );
    expect(await screen.findByText("打开中…")).toBeInTheDocument();
    resolveOpen({
      ok: true,
      alias: "UTS",
      remoteSessionId: "01a01907-adf3-7e00-a7a8-aee1082b0556",
      appSessionId: "app-1",
    });
    await waitFor(() => {
      expect(onOpenSession).toHaveBeenCalledWith("app-1");
    });
  });

  it("opens remote transcript into a local session", async () => {
    vi.mocked(api.sshOpenSession).mockResolvedValue({
      ok: true,
      alias: "UTS",
      remoteSessionId: "01a01907-adf3-7e00-a7a8-aee1082b0556",
      appSessionId: "app-1",
      title: "qwen35-v001-light",
      messageCount: 4,
    });
    render(<SshRemoteSessionRail t={t} onOpenSession={onOpenSession} />);
    await userEvent.click(
      screen.getByRole("button", { name: "qwen35-v001-light" }),
    );
    await waitFor(() => {
      expect(api.sshOpenSession).toHaveBeenCalled();
      expect(onOpenSession).toHaveBeenCalledWith("app-1");
    });
  });
});
