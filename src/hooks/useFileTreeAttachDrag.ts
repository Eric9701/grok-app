/**
 * Pointer-drag a files-tree row onto the composer (or the inline edit box)
 * to attach that file or folder.
 *
 * Not HTML5 drag. A page-started `dragstart` that aborts leaves WebView2
 * ignoring the mouse and keyboard (`windowsDragInputGuard`). Sidebar session
 * attach uses the same pointer-ghost approach.
 *
 * Chrome is pure DOM — no React setState during the gesture — so the
 * virtualized tree stays put. Capture waits until the pointer moves past
 * the threshold, so a click still opens the file or toggles the folder.
 */
import { useEffect, useRef } from "react";
import { joinProjectPath } from "@/lib/sideWorkbench";

/** Travel before a press becomes a drag. Click jitter stays under this. */
export const FILE_TREE_DRAG_THRESHOLD_PX = 16;
/**
 * First pointermove after down is often a synthetic jump. Rebase once,
 * then a later move in any direction can arm. The composer sits beside
 * the tree, so a sideways pull must count.
 */
export const FILE_TREE_DRAG_HOLD_MS = 80;

export const FILE_TREE_DRAG_ROOT = "data-file-tree-drag";
export const FILE_TREE_ATTACH_TARGET = "data-composer-attach";

const ROW_DRAGGING = "is-dragging";
const DROP_TARGET = "is-composer-drop";
const DRAG_GHOST = "rp-tree__drag-ghost";
const HTML_DRAGGING = "is-file-tree-drag";

export type FileTreeDragArm = "ignore" | "rebase" | "arm";

export function fileTreeDragArmDecision(input: {
  dx: number;
  dy: number;
  elapsedMs: number;
  buttons: number;
  /** Already absorbed one synthetic jump. */
  rebased?: boolean;
  /** Window became key mid-press (activation remapped the coordinates). */
  activationDuring?: boolean;
}): FileTreeDragArm {
  if (input.buttons === 0) return "ignore";
  const dist = Math.hypot(input.dx, input.dy);
  const settling =
    input.activationDuring ||
    (!input.rebased && input.elapsedMs < FILE_TREE_DRAG_HOLD_MS);
  if (settling) {
    return dist >= FILE_TREE_DRAG_THRESHOLD_PX ? "rebase" : "ignore";
  }
  return dist >= FILE_TREE_DRAG_THRESHOLD_PX ? "arm" : "ignore";
}

/** Absolute path under the project root, or null when the relative path escapes. */
export function fileTreeAttachAbsolutePath(
  projectRoot: string,
  relative: string,
): string | null {
  const root = projectRoot.trim();
  let rel = relative.trim();
  if (!root || !rel) return null;
  const rootNorm = root.replace(/\\/g, "/").replace(/\/+$/, "");
  const asPosix = rel.replace(/\\/g, "/");
  if (asPosix === rootNorm || asPosix.startsWith(`${rootNorm}/`)) {
    rel = asPosix.slice(rootNorm.length).replace(/^\/+/, "");
  }
  const parts = rel.replace(/\\/g, "/").split("/");
  if (parts.some((seg) => !seg || seg === "." || seg === "..")) return null;
  if (/^[A-Za-z]:/.test(rel) || rel.startsWith("/") || rel.startsWith("\\")) {
    return null;
  }
  return joinProjectPath(root, rel);
}

/** Composer or inline-edit box under the pointer, ignoring the drag ghost. */
export function fileTreeAttachTargetFromElements(
  stack: ArrayLike<Element>,
): HTMLElement | null {
  for (let i = 0; i < stack.length; i++) {
    const el = stack[i];
    if (!(el instanceof Element)) continue;
    if (el.classList.contains(DRAG_GHOST)) continue;
    const hit = el.closest<HTMLElement>(`[${FILE_TREE_ATTACH_TARGET}]`);
    if (hit) return hit;
  }
  return null;
}

export function fileTreeAttachTargetFromPoint(
  clientX: number,
  clientY: number,
): HTMLElement | null {
  const fromPoint = document.elementsFromPoint;
  if (typeof fromPoint !== "function") return null;
  return fileTreeAttachTargetFromElements(fromPoint.call(document, clientX, clientY));
}

function pageHasFocus(): boolean {
  if (typeof document.hasFocus !== "function") return true;
  try {
    return document.hasFocus();
  } catch {
    return true;
  }
}

function removeGhost(ghost: HTMLElement | null) {
  if (!ghost) return;
  try {
    ghost.remove();
  } catch {
    /* ignore */
  }
}

function clearDropClasses() {
  document.querySelectorAll(`.${DROP_TARGET}`).forEach((el) => {
    el.classList.remove(DROP_TARGET);
  });
}

function clearDraggingClasses() {
  document.querySelectorAll(`.rp-tree__row.${ROW_DRAGGING}`).forEach((el) => {
    el.classList.remove(ROW_DRAGGING);
  });
  document.documentElement.classList.remove(HTML_DRAGGING);
}

function moveGhost(
  ghost: HTMLElement,
  clientX: number,
  clientY: number,
  offsetX: number,
  offsetY: number,
) {
  ghost.style.left = `${clientX - offsetX}px`;
  ghost.style.top = `${clientY - offsetY}px`;
}

function createFileTreeDragGhost(
  label: string,
  clientX: number,
  clientY: number,
): { ghost: HTMLElement; offsetX: number; offsetY: number } {
  const ghost = document.createElement("div");
  ghost.className = DRAG_GHOST;
  ghost.setAttribute("aria-hidden", "true");
  ghost.textContent = label;
  const offsetX = 16;
  const offsetY = 12;
  ghost.style.position = "fixed";
  ghost.style.left = `${clientX - offsetX}px`;
  ghost.style.top = `${clientY - offsetY}px`;
  ghost.style.zIndex = "80";
  ghost.style.pointerEvents = "none";
  document.body.appendChild(ghost);
  return { ghost, offsetX, offsetY };
}

export function useFileTreeAttachDrag(opts: {
  /** Local project only. SSH listings are not local paths the attach API can classify. */
  enabled: boolean;
  projectPath: string | null;
  onAttach?: (paths: string[]) => void;
}): void {
  const { enabled, projectPath, onAttach } = opts;
  const enabledRef = useRef(enabled);
  enabledRef.current = enabled;
  const projectPathRef = useRef(projectPath);
  projectPathRef.current = projectPath;
  const onAttachRef = useRef(onAttach);
  onAttachRef.current = onAttach;

  const sessionRef = useRef<{
    pointerId: number;
    relativePath: string;
    label: string;
    startX: number;
    startY: number;
    startTime: number;
    rebased: boolean;
    active: boolean;
    activationDuring: boolean;
    captureEl: HTMLElement | null;
    ghost: HTMLElement | null;
    ghostOffsetX: number;
    ghostOffsetY: number;
    overTarget: boolean;
  } | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    const endSession = (commit: boolean) => {
      const s = sessionRef.current;
      sessionRef.current = null;
      cleanupRef.current?.();
      cleanupRef.current = null;
      removeGhost(s?.ghost ?? null);
      clearDropClasses();
      clearDraggingClasses();
      if (s?.captureEl) {
        try {
          if (s.captureEl.hasPointerCapture?.(s.pointerId)) {
            s.captureEl.releasePointerCapture(s.pointerId);
          }
        } catch {
          /* ignore */
        }
      }
      if (!s?.active) return;

      const blockClick = (ev: Event) => {
        ev.stopPropagation();
        ev.preventDefault();
        window.removeEventListener("click", blockClick, true);
      };
      window.addEventListener("click", blockClick, true);
      window.setTimeout(() => {
        window.removeEventListener("click", blockClick, true);
      }, 400);

      if (!commit || !s.overTarget) return;
      const abs = fileTreeAttachAbsolutePath(
        projectPathRef.current || "",
        s.relativePath,
      );
      if (!abs) return;
      onAttachRef.current?.([abs]);
    };

    const onDown = (e: PointerEvent) => {
      if (!enabledRef.current) return;
      if (e.button !== 0) return;
      if (sessionRef.current) return;
      if (!pageHasFocus()) return;
      const raw = e.target;
      if (!(raw instanceof Element)) return;
      const row = raw.closest<HTMLElement>(".rp-tree__row[data-tree-path]");
      if (!row || !row.closest(`[${FILE_TREE_DRAG_ROOT}]`)) return;
      const relativePath = row.dataset.treePath?.trim() || "";
      if (!relativePath) return;
      const label = row.dataset.treeName?.trim() || relativePath;
      const pointerId = e.pointerId;

      sessionRef.current = {
        pointerId,
        relativePath,
        label,
        startX: e.clientX,
        startY: e.clientY,
        startTime: e.timeStamp,
        rebased: false,
        active: false,
        activationDuring: false,
        captureEl: row,
        ghost: null,
        ghostOffsetX: 0,
        ghostOffsetY: 0,
        overTarget: false,
      };

      const onMove = (ev: PointerEvent) => {
        const s = sessionRef.current;
        if (!s || ev.pointerId !== s.pointerId) return;
        const dx = ev.clientX - s.startX;
        const dy = ev.clientY - s.startY;
        if (!s.active) {
          const decision = fileTreeDragArmDecision({
            dx,
            dy,
            elapsedMs: ev.timeStamp - s.startTime,
            buttons: ev.buttons,
            rebased: s.rebased,
            activationDuring: s.activationDuring,
          });
          if (decision === "ignore") return;
          if (decision === "rebase") {
            s.startX = ev.clientX;
            s.startY = ev.clientY;
            s.rebased = true;
            s.activationDuring = false;
            return;
          }
          s.active = true;
          try {
            row.setPointerCapture?.(pointerId);
          } catch {
            /* older WebView */
          }
          try {
            ev.preventDefault();
          } catch {
            /* ignore */
          }
          document.documentElement.classList.add(HTML_DRAGGING);
          try {
            const built = createFileTreeDragGhost(s.label, ev.clientX, ev.clientY);
            s.ghost = built.ghost;
            s.ghostOffsetX = built.offsetX;
            s.ghostOffsetY = built.offsetY;
          } catch {
            s.ghost = null;
          }
          row.classList.add(ROW_DRAGGING);
        } else {
          try {
            ev.preventDefault();
          } catch {
            /* ignore */
          }
          if (s.ghost) {
            moveGhost(
              s.ghost,
              ev.clientX,
              ev.clientY,
              s.ghostOffsetX,
              s.ghostOffsetY,
            );
          }
        }

        const hit = fileTreeAttachTargetFromPoint(ev.clientX, ev.clientY);
        clearDropClasses();
        s.overTarget = !!hit;
        if (hit) hit.classList.add(DROP_TARGET);
      };

      const onUp = (ev: PointerEvent) => {
        const s = sessionRef.current;
        if (!s || ev.pointerId !== s.pointerId) return;
        endSession(s.active);
      };

      const onCancel = (ev: PointerEvent) => {
        const s = sessionRef.current;
        if (!s || ev.pointerId !== s.pointerId) return;
        endSession(false);
      };

      const onKey = (ev: KeyboardEvent) => {
        if (ev.key !== "Escape") return;
        if (!sessionRef.current) return;
        ev.preventDefault();
        endSession(false);
      };

      const onWinFocus = () => {
        const s = sessionRef.current;
        if (!s || s.active) return;
        s.activationDuring = true;
      };

      window.addEventListener("pointermove", onMove, true);
      window.addEventListener("pointerup", onUp, true);
      window.addEventListener("pointercancel", onCancel, true);
      window.addEventListener("keydown", onKey, true);
      window.addEventListener("focus", onWinFocus);
      cleanupRef.current = () => {
        window.removeEventListener("pointermove", onMove, true);
        window.removeEventListener("pointerup", onUp, true);
        window.removeEventListener("pointercancel", onCancel, true);
        window.removeEventListener("keydown", onKey, true);
        window.removeEventListener("focus", onWinFocus);
      };
    };

    document.addEventListener("pointerdown", onDown);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      endSession(false);
    };
  }, []);
}
