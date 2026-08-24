/**
 * @vitest-environment jsdom
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SshRemoteSessionRail } from "./SshRemoteSessionRail";
import type { MessageKey, Vars } from "@/i18n";

const loadMore = vi.fn();
const newChat = vi.fn();
const setDraftRemote = vi.fn();
const renameRemoteSession = vi.fn();

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
  return String(k);
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SshRemoteSessionRail", () => {
  it("labels the host, hides the raw UUID, and offers load more", async () => {
    render(<SshRemoteSessionRail t={t} newChat={newChat} />);
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
});
