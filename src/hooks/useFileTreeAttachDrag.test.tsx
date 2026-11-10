/**
 * @vitest-environment jsdom
 */
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  FILE_TREE_DRAG_HOLD_MS,
  FILE_TREE_DRAG_THRESHOLD_PX,
  fileTreeAttachAbsolutePath,
  fileTreeAttachTargetFromElements,
  fileTreeDragArmDecision,
  useFileTreeAttachDrag,
} from "./useFileTreeAttachDrag";

afterEach(() => {
  cleanup();
  document.documentElement.classList.remove("is-file-tree-drag");
  vi.restoreAllMocks();
});

describe("fileTreeDragArmDecision", () => {
  it("keeps a click-sized jitter as a click", () => {
    expect(
      fileTreeDragArmDecision({ dx: 3, dy: 2, elapsedMs: 200, buttons: 1 }),
    ).toBe("ignore");
    expect(
      fileTreeDragArmDecision({ dx: 0, dy: 0, elapsedMs: 0, buttons: 1 }),
    ).toBe("ignore");
  });

  it("rebases one early jump, then arms a sideways pull", () => {
    expect(
      fileTreeDragArmDecision({
        dx: 40,
        dy: 0,
        elapsedMs: 10,
        buttons: 1,
      }),
    ).toBe("rebase");
    expect(
      fileTreeDragArmDecision({
        dx: FILE_TREE_DRAG_THRESHOLD_PX,
        dy: 4,
        elapsedMs: 20,
        buttons: 1,
        rebased: true,
      }),
    ).toBe("arm");
  });

  it("arms any direction once the hold has passed", () => {
    expect(
      fileTreeDragArmDecision({
        dx: 0,
        dy: FILE_TREE_DRAG_THRESHOLD_PX,
        elapsedMs: FILE_TREE_DRAG_HOLD_MS,
        buttons: 1,
      }),
    ).toBe("arm");
    expect(
      fileTreeDragArmDecision({
        dx: FILE_TREE_DRAG_THRESHOLD_PX,
        dy: 0,
        elapsedMs: FILE_TREE_DRAG_HOLD_MS,
        buttons: 1,
      }),
    ).toBe("arm");
  });

  it("ignores a move after the button is up", () => {
    expect(
      fileTreeDragArmDecision({
        dx: 40,
        dy: 40,
        elapsedMs: 200,
        buttons: 0,
      }),
    ).toBe("ignore");
  });
});

describe("fileTreeAttachAbsolutePath", () => {
  it("joins a project-relative file and keeps the root's separators", () => {
    expect(fileTreeAttachAbsolutePath("/repo", "src/notes.md")).toBe(
      "/repo/src/notes.md",
    );
    expect(fileTreeAttachAbsolutePath("D:\\work", "src/notes.md")).toBe(
      "D:\\work\\src\\notes.md",
    );
  });

  it("accepts a path that already includes the root", () => {
    expect(fileTreeAttachAbsolutePath("/repo", "/repo/src/a.ts")).toBe(
      "/repo/src/a.ts",
    );
  });

  it("rejects paths that leave the project", () => {
    expect(fileTreeAttachAbsolutePath("/repo", "../etc/passwd")).toBeNull();
    expect(fileTreeAttachAbsolutePath("/repo", "src/../../etc")).toBeNull();
    expect(fileTreeAttachAbsolutePath("/repo", "")).toBeNull();
    expect(fileTreeAttachAbsolutePath("", "a.ts")).toBeNull();
  });
});

describe("fileTreeAttachTargetFromElements", () => {
  it("prefers the composer and skips the ghost", () => {
    const ghost = document.createElement("div");
    ghost.className = "rp-tree__drag-ghost";
    const composer = document.createElement("div");
    composer.setAttribute("data-composer-attach", "");
    const inner = document.createElement("textarea");
    composer.appendChild(inner);
    expect(fileTreeAttachTargetFromElements([ghost, inner])).toBe(composer);
    expect(fileTreeAttachTargetFromElements([ghost])).toBeNull();
  });
});

function Harness({
  enabled = true,
  projectPath = "/repo",
  onAttach,
  onOpen,
}: {
  enabled?: boolean;
  projectPath?: string | null;
  onAttach?: (paths: string[]) => void;
  onOpen?: () => void;
}) {
  useFileTreeAttachDrag({ enabled, projectPath, onAttach });
  return (
    <div data-file-tree-drag="">
      <button
        type="button"
        className="rp-tree__row"
        data-tree-path="src/notes.md"
        data-tree-name="notes.md"
        onClick={onOpen}
      >
        notes.md
      </button>
    </div>
  );
}

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
    buttons: type === "pointerup" || type === "pointercancel" ? 0 : 1,
    ...init,
  });
  if (typeof init.timeStamp === "number") {
    Object.defineProperty(ev, "timeStamp", { value: init.timeStamp });
  }
  target.dispatchEvent(ev);
}

describe("useFileTreeAttachDrag", () => {
  it("a press without travel still opens the row", () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const onOpen = vi.fn();
    const onAttach = vi.fn();
    const { getByRole } = render(
      <Harness onOpen={onOpen} onAttach={onAttach} />,
    );
    const row = getByRole("button");
    dispatchPointer("pointerdown", row, { clientX: 20, clientY: 20, timeStamp: 0 });
    dispatchPointer("pointerup", window, { clientX: 22, clientY: 21, timeStamp: 40 });
    fireEvent.click(row);
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onAttach).not.toHaveBeenCalled();
  });

  it("drops a sideways drag on the composer as an absolute path", () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const composer = document.createElement("div");
    composer.className = "composer";
    composer.setAttribute("data-composer-attach", "");
    document.body.appendChild(composer);
    const onAttach = vi.fn();
    const onOpen = vi.fn();
    const { getByRole } = render(
      <Harness onOpen={onOpen} onAttach={onAttach} />,
    );
    const row = getByRole("button");
    stubElementsFromPoint([composer]);
    dispatchPointer("pointerdown", row, { clientX: 400, clientY: 80, timeStamp: 0 });
    dispatchPointer("pointermove", window, {
      clientX: 360,
      clientY: 80,
      timeStamp: 10,
    });
    dispatchPointer("pointermove", window, {
      clientX: 200,
      clientY: 200,
      timeStamp: 40,
    });
    expect(composer.classList.contains("is-composer-drop")).toBe(true);
    expect(document.querySelector(".rp-tree__drag-ghost")?.textContent).toBe(
      "notes.md",
    );
    dispatchPointer("pointerup", window, {
      clientX: 200,
      clientY: 200,
      timeStamp: 80,
    });
    expect(onAttach).toHaveBeenCalledWith(["/repo/src/notes.md"]);
    expect(document.querySelector(".rp-tree__drag-ghost")).toBeNull();
    expect(composer.classList.contains("is-composer-drop")).toBe(false);
    fireEvent.click(row);
    expect(onOpen).not.toHaveBeenCalled();
    composer.remove();
  });

  it("does not attach when the pointer is released off the composer", () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    stubElementsFromPoint([]);
    const onAttach = vi.fn();
    const { getByRole } = render(<Harness onAttach={onAttach} />);
    const row = getByRole("button");
    dispatchPointer("pointerdown", row, { clientX: 10, clientY: 10, timeStamp: 0 });
    dispatchPointer("pointermove", window, {
      clientX: 40,
      clientY: 10,
      timeStamp: FILE_TREE_DRAG_HOLD_MS,
    });
    dispatchPointer("pointerup", window, {
      clientX: 40,
      clientY: 10,
      timeStamp: FILE_TREE_DRAG_HOLD_MS + 20,
    });
    expect(onAttach).not.toHaveBeenCalled();
  });

  it("escape cancels an armed drag", () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const composer = document.createElement("div");
    composer.setAttribute("data-composer-attach", "");
    document.body.appendChild(composer);
    stubElementsFromPoint([composer]);
    const onAttach = vi.fn();
    const { getByRole } = render(<Harness onAttach={onAttach} />);
    const row = getByRole("button");
    dispatchPointer("pointerdown", row, { clientX: 10, clientY: 10, timeStamp: 0 });
    dispatchPointer("pointermove", window, {
      clientX: 40,
      clientY: 10,
      timeStamp: FILE_TREE_DRAG_HOLD_MS,
    });
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
    );
    dispatchPointer("pointerup", window, {
      clientX: 40,
      clientY: 12,
      timeStamp: FILE_TREE_DRAG_HOLD_MS + 30,
    });
    expect(onAttach).not.toHaveBeenCalled();
    expect(document.querySelector(".rp-tree__drag-ghost")).toBeNull();
    composer.remove();
  });

  it("does nothing while disabled", () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const onAttach = vi.fn();
    const { getByRole } = render(
      <Harness enabled={false} onAttach={onAttach} />,
    );
    const row = getByRole("button");
    dispatchPointer("pointerdown", row, { clientX: 10, clientY: 10, timeStamp: 0 });
    dispatchPointer("pointermove", window, {
      clientX: 80,
      clientY: 40,
      timeStamp: 200,
    });
    dispatchPointer("pointerup", window, { clientX: 80, clientY: 40, timeStamp: 220 });
    expect(onAttach).not.toHaveBeenCalled();
    expect(document.querySelector(".rp-tree__drag-ghost")).toBeNull();
  });
});
