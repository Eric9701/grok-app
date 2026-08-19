import { describe, expect, it } from "vitest";
import {
  isPastSessionDragThreshold,
  isSessionMoveIgnoredTarget,
  SESSION_DRAG_THRESHOLD_PX,
  sessionDragDropFromElements,
} from "@/hooks/useSidebarSessionMoveDrag";

function rowWithChrome() {
  const actionBtn = document.createElement("button");
  actionBtn.className = "tree-icon-btn";
  const glyph = document.createElement("span");
  actionBtn.appendChild(glyph);
  const actions = document.createElement("span");
  actions.className = "tree-l3__actions";
  actions.appendChild(actionBtn);
  const row = document.createElement("div");
  row.className = "tree-l3";
  const title = document.createElement("span");
  title.className = "tree-l3__title";
  title.textContent = "Chat";
  row.appendChild(title);
  row.appendChild(actions);
  return { actionBtn, glyph, row, title };
}

describe("sidebar attach vs move gestures", () => {
  it("row-body starts move; action chrome does not", () => {
    if (typeof document === "undefined") return;
    const { actionBtn, glyph, row, title } = rowWithChrome();
    expect(isSessionMoveIgnoredTarget(actionBtn)).toBe(true);
    expect(isSessionMoveIgnoredTarget(glyph)).toBe(true);
    expect(isSessionMoveIgnoredTarget(row)).toBe(false);
    expect(isSessionMoveIgnoredTarget(title)).toBe(false);
    expect(document.querySelector(".tree-l3__drag-handle")).toBeNull();
  });

  it("does not treat a click-sized jitter as a drag", () => {
    expect(isPastSessionDragThreshold(0, 0)).toBe(false);
    expect(isPastSessionDragThreshold(3, 3)).toBe(false);
    expect(isPastSessionDragThreshold(SESSION_DRAG_THRESHOLD_PX - 1, 0)).toBe(
      false,
    );
    expect(isPastSessionDragThreshold(SESSION_DRAG_THRESHOLD_PX, 0)).toBe(true);
    expect(isPastSessionDragThreshold(0, SESSION_DRAG_THRESHOLD_PX)).toBe(true);
  });

  it("prefers composer attach over project move in the hit stack", () => {
    if (typeof document === "undefined") return;
    const ghost = document.createElement("div");
    ghost.className = "tree-l3 tree-l3--drag-ghost";
    const composer = document.createElement("div");
    composer.setAttribute("data-session-attach", "");
    const inner = document.createElement("div");
    composer.appendChild(inner);
    const project = document.createElement("div");
    project.setAttribute("data-session-drop", "proj-a");
    const orphan = document.createElement("div");
    orphan.setAttribute("data-session-drop", "__orphan__");

    expect(sessionDragDropFromElements([ghost, inner])).toEqual({
      kind: "attach",
      node: composer,
    });
    expect(sessionDragDropFromElements([inner, project])).toEqual({
      kind: "attach",
      node: composer,
    });
    expect(sessionDragDropFromElements([project])).toEqual({
      kind: "move",
      node: project,
      projectId: "proj-a",
    });
    expect(sessionDragDropFromElements([orphan])).toEqual({
      kind: "move",
      node: orphan,
      projectId: null,
    });
    expect(sessionDragDropFromElements([ghost])).toEqual({ kind: "none" });
  });
});
