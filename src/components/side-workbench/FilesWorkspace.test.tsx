/**
 * @vitest-environment jsdom
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@/test/jsdomStubs";
import type { SessionFileChange } from "@/lib/sessionChanges";
import { FilesWorkspace } from "./FilesWorkspace";

const apiMocks = vi.hoisted(() => ({
  fsListDir: vi.fn(),
  sshListDir: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  isTauri: () => true,
  fsListDir: apiMocks.fsListDir,
  sshListDir: apiMocks.sshListDir,
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
}));

vi.mock("@/components/OpenLocationButton", () => ({
  OpenLocationButton: () => null,
}));

vi.mock("@/components/ui/tooltip", () => ({
  Tip: ({ children }: { children: React.ReactNode }) => children,
}));

vi.mock("@/components/resource-viewer/useResourceFileTabs", () => ({
  useResourceFileTabs: () => ({
    activeTab: null,
    openFile: vi.fn(),
    openAbsoluteFile: vi.fn(),
    updateActiveDraft: vi.fn(),
    saveActiveFile: vi.fn(),
    revertActiveDraft: vi.fn(),
    toggleActiveEditMode: vi.fn(),
    filesTabsEmpty: null,
    dirtyPaths: [],
    closeByPath: vi.fn(() => true),
    discardTabId: null,
    setDiscardTabId: vi.fn(),
    closeTabForced: vi.fn(),
    conflictTabId: null,
    setConflictTabId: vi.fn(),
    reloadActiveFile: vi.fn(),
  }),
}));

const dir = (name: string) => ({
  name,
  relativePath: name,
  isDir: true,
  size: 0,
  ext: "",
});

const file = (path: string) => ({
  name: path.split("/").at(-1) || path,
  relativePath: path,
  isDir: false,
  size: 1,
  ext: "ts",
});

const change = (
  path: string,
  updatedAt: string,
  status = "completed",
  toolCallId = `call:${path}`,
): SessionFileChange => ({
  path,
  name: path.split("/").at(-1) || path,
  toolKind: "write",
  status,
  updatedAt,
  toolCallId,
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  apiMocks.fsListDir.mockReset();
  apiMocks.sshListDir.mockReset();
});

describe("FilesWorkspace session-change refresh", () => {
  it("refreshes once per path set, catches up after hidden, and preserves expanded children on failure", async () => {
    let phase = 0;
    apiMocks.fsListDir.mockImplementation(
      async (_projectPath: string, relativePath: string) => {
        if (!relativePath) {
          return [dir("src"), ...(phase > 0 ? [file("README.md")] : [])];
        }
        if (relativePath !== "src") return [];
        if (phase === 3) throw new Error("nested read failed");
        return [
          file("src/old.ts"),
          ...(phase > 0 ? [file("src/fresh.ts")] : []),
          ...(phase > 1 ? [file("src/hidden.ts")] : []),
        ];
      },
    );

    const props = {
      locale: "en",
      projectPath: "/repo",
      projectName: "repo",
      treeVisible: true,
      onTreeVisibleChange: vi.fn(),
      paneActive: true,
      sessionChanges: [] as SessionFileChange[],
    };
    const view = render(<FilesWorkspace {...props} />);

    const src = await screen.findByText("src");
    await userEvent.click(src.closest("button")!);
    expect(await screen.findByText("old.ts")).toBeInTheDocument();
    expect(apiMocks.fsListDir).toHaveBeenCalledTimes(2);

    phase = 1;
    view.rerender(
      <FilesWorkspace
        {...props}
        sessionChanges={[
          change(
            "src/fresh.ts",
            "2026-08-25T01:00:00Z",
            "in_progress",
            "call:fresh",
          ),
        ]}
      />,
    );
    await waitFor(() => expect(apiMocks.fsListDir).toHaveBeenCalledTimes(2));
    expect(screen.queryByText("fresh.ts")).not.toBeInTheDocument();

    view.rerender(
      <FilesWorkspace
        {...props}
        sessionChanges={[
          change(
            "src/fresh.ts",
            "2026-08-25T01:00:01Z",
            "completed",
            "call:fresh",
          ),
        ]}
      />,
    );
    expect(await screen.findByText("fresh.ts")).toBeInTheDocument();
    expect(apiMocks.fsListDir).toHaveBeenCalledTimes(4);

    view.rerender(
      <FilesWorkspace
        {...props}
        sessionChanges={[
          change(
            "src/fresh.ts",
            "2026-08-25T02:00:00Z",
            "completed",
            "call:fresh",
          ),
        ]}
      />,
    );
    await waitFor(() => expect(apiMocks.fsListDir).toHaveBeenCalledTimes(4));

    phase = 2;
    view.rerender(
      <FilesWorkspace
        {...props}
        paneActive={false}
        sessionChanges={[change("src/hidden.ts", "2026-08-25T03:00:00Z")]}
      />,
    );
    await waitFor(() => expect(apiMocks.fsListDir).toHaveBeenCalledTimes(4));

    view.rerender(
      <FilesWorkspace
        {...props}
        sessionChanges={[change("src/hidden.ts", "2026-08-25T03:00:00Z")]}
      />,
    );
    expect(await screen.findByText("hidden.ts")).toBeInTheDocument();
    expect(apiMocks.fsListDir).toHaveBeenCalledTimes(6);

    phase = 3;
    view.rerender(
      <FilesWorkspace
        {...props}
        sessionChanges={[change("src/failed.ts", "2026-08-25T04:00:00Z")]}
      />,
    );
    await waitFor(() => expect(apiMocks.fsListDir).toHaveBeenCalledTimes(8));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "nested read failed",
    );
    expect(screen.getByText("hidden.ts")).toBeInTheDocument();
  });

  it("discards an older soft refresh when a newer completed revision wins", async () => {
    let resolveStale!: (value: ReturnType<typeof file>[]) => void;
    const staleRead = new Promise<ReturnType<typeof file>[]>((resolve) => {
      resolveStale = resolve;
    });
    let rootReads = 0;
    apiMocks.fsListDir.mockImplementation(
      async (_projectPath: string, relativePath: string) => {
        if (relativePath) return [];
        rootReads += 1;
        if (rootReads === 1) return [file("base.ts")];
        if (rootReads === 2) return staleRead;
        return [file("latest.ts")];
      },
    );

    const props = {
      locale: "en",
      projectPath: "/repo",
      projectName: "repo",
      treeVisible: true,
      onTreeVisibleChange: vi.fn(),
      paneActive: true,
      sessionChanges: [] as SessionFileChange[],
    };
    const view = render(<FilesWorkspace {...props} />);
    expect(await screen.findByText("base.ts")).toBeInTheDocument();

    view.rerender(
      <FilesWorkspace
        {...props}
        sessionChanges={[
          change("same.ts", "2026-08-25T05:00:00Z", "completed", "call:1"),
        ]}
      />,
    );
    await waitFor(() => expect(apiMocks.fsListDir).toHaveBeenCalledTimes(2));

    view.rerender(
      <FilesWorkspace
        {...props}
        sessionChanges={[
          change("same.ts", "2026-08-25T05:00:01Z", "completed", "call:2"),
        ]}
      />,
    );
    expect(await screen.findByText("latest.ts")).toBeInTheDocument();

    await act(async () => {
      resolveStale([file("stale.ts")]);
      await staleRead;
    });
    await waitFor(() => {
      expect(screen.getByText("latest.ts")).toBeInTheDocument();
      expect(screen.queryByText("stale.ts")).not.toBeInTheDocument();
    });
  });
});

function stubElementsFromPoint(stack: Element[]) {
  Object.defineProperty(document, "elementsFromPoint", {
    configurable: true,
    value: () => stack,
  });
}

function dispatchPointer(
  type: string,
  target: EventTarget,
  init: PointerEventInit & { timeStamp?: number },
) {
  const ev = new PointerEvent(type, {
    bubbles: true,
    cancelable: true,
    pointerId: 1,
    button: 0,
    buttons: type === "pointerup" ? 0 : 1,
    ...init,
  });
  if (typeof init.timeStamp === "number") {
    Object.defineProperty(ev, "timeStamp", { value: init.timeStamp });
  }
  target.dispatchEvent(ev);
}

describe("FilesWorkspace file-tree drag", () => {
  it("attaches a local file dropped on the composer and still opens on click", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    apiMocks.fsListDir.mockResolvedValue([file("notes.md")]);
    const onAttachPaths = vi.fn();
    const onFileOpen = vi.fn();
    render(
      <FilesWorkspace
        locale="en"
        projectPath="/repo"
        projectName="repo"
        treeVisible
        onTreeVisibleChange={vi.fn()}
        paneActive
        sessionChanges={[]}
        onAttachPaths={onAttachPaths}
        onFileOpen={onFileOpen}
      />,
    );
    const row = (await screen.findByText("notes.md")).closest("button");
    expect(row).toHaveAttribute("data-tree-path", "notes.md");
    if (!row) throw new Error("row missing");

    dispatchPointer("pointerdown", row, { clientX: 20, clientY: 20, timeStamp: 0 });
    dispatchPointer("pointerup", window, { clientX: 22, clientY: 21, timeStamp: 30 });
    row.click();
    await waitFor(() =>
      expect(onFileOpen).toHaveBeenCalledWith("notes.md", "notes.md"),
    );
    expect(onAttachPaths).not.toHaveBeenCalled();

    const composer = document.createElement("div");
    composer.setAttribute("data-composer-attach", "");
    document.body.appendChild(composer);
    stubElementsFromPoint([composer]);
    dispatchPointer("pointerdown", row, { clientX: 300, clientY: 40, timeStamp: 0 });
    dispatchPointer("pointermove", window, {
      clientX: 260,
      clientY: 40,
      timeStamp: 10,
    });
    dispatchPointer("pointermove", window, {
      clientX: 80,
      clientY: 120,
      timeStamp: 50,
    });
    dispatchPointer("pointerup", window, {
      clientX: 80,
      clientY: 120,
      timeStamp: 70,
    });
    expect(onAttachPaths).toHaveBeenCalledWith(["/repo/notes.md"]);
    composer.remove();
  });

  it("does not drag files from an SSH tree", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    apiMocks.sshListDir.mockResolvedValue({
      ok: true,
      entries: [{ name: "remote.md", isDir: false }],
    });
    const onAttachPaths = vi.fn();
    render(
      <FilesWorkspace
        locale="en"
        projectPath="/home/proj"
        sshAlias="box"
        projectName="proj"
        treeVisible
        onTreeVisibleChange={vi.fn()}
        paneActive
        sessionChanges={[]}
        onAttachPaths={onAttachPaths}
      />,
    );
    const row = (await screen.findByText("remote.md")).closest("button");
    if (!row) throw new Error("row missing");
    const composer = document.createElement("div");
    composer.setAttribute("data-composer-attach", "");
    document.body.appendChild(composer);
    stubElementsFromPoint([composer]);
    dispatchPointer("pointerdown", row, { clientX: 20, clientY: 20, timeStamp: 0 });
    dispatchPointer("pointermove", window, {
      clientX: 80,
      clientY: 80,
      timeStamp: 200,
    });
    dispatchPointer("pointerup", window, { clientX: 80, clientY: 80, timeStamp: 220 });
    expect(onAttachPaths).not.toHaveBeenCalled();
    composer.remove();
  });
});
