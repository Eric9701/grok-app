/**
 * Overlay chrome: living mark + task bubbles + in-window menu.
 * 编辑 opens the same Settings → 宠物 hash as the settings nav item.
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { PetMark } from "./PetMark";
import { PetTaskBubbles } from "./PetTaskBubbles";
import { createT, type Locale } from "@/i18n";
import {
  isPetColor,
  isPetShape,
  PET_BUBBLE_WIDTH,
  petOverlayHeight,
  petOverlayWidth,
  petSettingsHash,
  petVerbFor,
  type PetFocus,
  type PetTask,
} from "@/lib/pet";
import {
  petFocusSession,
  petHide,
  petOpenSettings,
  petReadBubbleOffset,
  petSetDragging,
  petSetHitChrome,
  petSetIgnoreCursor,
  petSetMenuOpen,
  petShowMain,
  petStartDragging,
  petSyncOverlaySize,
} from "@/lib/api/pet";
import type { PetPrefs } from "@/lib/api/pet";

export { petSettingsHash };

export function openPetSettingsFromMenu(): Promise<void> {
  return petOpenSettings();
}

const DRAG_SLOP = 6;
const DBLCLICK_MS = 280;

export function PetOverlay({
  focus,
  tasks = [],
  prefs,
  locale = "en",
}: {
  focus: PetFocus;
  tasks?: readonly PetTask[];
  prefs: PetPrefs;
  locale?: Locale;
}) {
  const t = useMemo(() => createT(locale), [locale]);
  const shape = isPetShape(prefs.shape) ? prefs.shape : "hex";
  const color = isPetColor(prefs.color) ? prefs.color : "green";
  const verb = petVerbFor(focus.kind, focus.toolTitle);
  const sizePx = prefs.sizePx || 128;
  const [menuOpen, setMenuOpen] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [bubbleDx, setBubbleDx] = useState(0);
  const originRef = useRef<{ x: number; y: number } | null>(null);
  const draggedRef = useRef(false);
  const pendingClickRef = useRef<number | null>(null);
  const overlayRef = useRef<HTMLDivElement | null>(null);
  const markRef = useRef<HTMLDivElement | null>(null);
  const stackRef = useRef<HTMLDivElement | null>(null);

  const statusLabel = useMemo(() => {
    switch (focus.kind) {
      case "needs_you":
        return t("pet.status.needsYou");
      case "error":
        return t("pet.status.error");
      case "ready":
        return t("pet.status.ready");
      case "working":
        return t("pet.status.working");
      case "connecting":
        return t("pet.status.connecting");
      default:
        return t("pet.status.idle");
    }
  }, [focus.kind, t]);

  const title = [statusLabel, focus.title, focus.toolTitle]
    .filter(Boolean)
    .join(" · ");

  const reportHitChrome = useCallback(() => {
    const overlay = overlayRef.current;
    const mark = markRef.current;
    if (!overlay || !mark) return;
    const o = overlay.getBoundingClientRect();
    const m = mark.getBoundingClientRect();
    const b = stackRef.current?.getBoundingClientRect();
    void petSetHitChrome({
      markCx: m.left + m.width / 2 - o.left,
      markCy: m.top + m.height / 2 - o.top,
      markR: Math.max(m.width, m.height) * 0.52,
      bubbleX: b ? b.left - o.left : 0,
      bubbleY: b ? b.top - o.top : 0,
      bubbleW: b ? b.width : 0,
      bubbleH: b ? b.height : 0,
      windowW: o.width,
      windowH: o.height,
    });
  }, []);

  const refreshBubbleOffset = useCallback(async () => {
    const max = Math.max(0, (petOverlayWidth(sizePx) - PET_BUBBLE_WIDTH) / 2 - 8);
    const dx = await petReadBubbleOffset(max);
    setBubbleDx((prev) => (Math.abs(prev - dx) < 1 ? prev : dx));
    reportHitChrome();
  }, [sizePx, reportHitChrome]);

  useLayoutEffect(() => {
    const w = petOverlayWidth(sizePx);
    const h = petOverlayHeight(sizePx, tasks.length);
    void petSyncOverlaySize(w, h).then(() => {
      void refreshBubbleOffset();
    });
  }, [tasks.length, sizePx, refreshBubbleOffset]);

  useEffect(() => {
    if (dragging) return;
    let gone = false;
    let unlisten: (() => void) | undefined;
    let timer: number | null = null;
    void (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        unlisten = await win.onMoved(() => {
          if (gone) return;
          if (timer != null) window.clearTimeout(timer);
          timer = window.setTimeout(() => {
            if (!gone) void refreshBubbleOffset();
          }, 80);
        });
      } catch {
        /* not Tauri */
      }
    })();
    return () => {
      gone = true;
      if (timer != null) window.clearTimeout(timer);
      unlisten?.();
    };
  }, [dragging, refreshBubbleOffset]);

  useEffect(() => {
    return () => {
      if (pendingClickRef.current != null) {
        window.clearTimeout(pendingClickRef.current);
      }
    };
  }, []);

  const closeMenu = useCallback(() => {
    setMenuOpen(false);
    void petSetMenuOpen(false);
  }, []);

  const openTask = useCallback((sessionId: string) => {
    if (pendingClickRef.current != null) {
      window.clearTimeout(pendingClickRef.current);
      pendingClickRef.current = null;
    }
    void petFocusSession(sessionId);
  }, []);

  const openMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setMenuOpen(true);
    void petSetIgnoreCursor(false);
    void petSetMenuOpen(true);
  }, []);

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0) return;
      if ((e.target as HTMLElement | null)?.closest?.(".pet-menu, .pet-bubbles")) {
        return;
      }
      if (menuOpen) {
        closeMenu();
        return;
      }
      originRef.current = { x: e.screenX, y: e.screenY };
      draggedRef.current = false;
      try {
        e.currentTarget.setPointerCapture(e.pointerId);
      } catch {
        /* capture unsupported */
      }
    },
    [closeMenu, menuOpen],
  );

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    const start = originRef.current;
    if (!start || draggedRef.current) return;
    const dx = e.screenX - start.x;
    const dy = e.screenY - start.y;
    if (dx * dx + dy * dy < DRAG_SLOP * DRAG_SLOP) return;
    draggedRef.current = true;
    setDragging(true);
    void petSetIgnoreCursor(false);
    void petSetDragging(true);
    void petStartDragging().catch(() => {
      /* startDragging unavailable outside Tauri */
    });
  }, []);

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      const start = originRef.current;
      const moved = draggedRef.current;
      originRef.current = null;
      draggedRef.current = false;
      setDragging(false);
      void petSetDragging(false);
      try {
        e.currentTarget.releasePointerCapture(e.pointerId);
      } catch {
        /* ignore */
      }
      if (moved) {
        void refreshBubbleOffset();
        return;
      }
      if (!start) return;
      const dx = e.screenX - start.x;
      const dy = e.screenY - start.y;
      if (dx * dx + dy * dy >= DRAG_SLOP * DRAG_SLOP) return;
      if (pendingClickRef.current != null) {
        window.clearTimeout(pendingClickRef.current);
        pendingClickRef.current = null;
        void petShowMain();
        return;
      }
      pendingClickRef.current = window.setTimeout(() => {
        pendingClickRef.current = null;
        if (focus.sessionId) void petFocusSession(focus.sessionId);
        else void petShowMain();
      }, DBLCLICK_MS);
    },
    [focus.sessionId, refreshBubbleOffset],
  );

  return (
    <div
      ref={overlayRef}
      className={
        "pet-overlay" +
        (dragging ? " is-dragging" : "") +
        (tasks.length ? " has-bubbles" : "")
      }
      onContextMenu={openMenu}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        originRef.current = null;
        draggedRef.current = false;
        setDragging(false);
        void petSetDragging(false);
      }}
    >
      <div
        ref={stackRef}
        className="pet-bubbles-slot"
        style={{ transform: `translateX(${bubbleDx}px)` }}
      >
        <PetTaskBubbles tasks={tasks} t={t} onOpen={openTask} />
      </div>
      <div ref={markRef} className="pet-overlay__hit">
        <PetMark
          shape={shape}
          color={color}
          verb={verb}
          sizePx={sizePx}
          title={title}
        />
      </div>
      {menuOpen ? (
        <div
          className="pet-menu menu-panel context-menu att-menu"
          role="menu"
          onPointerDown={(e) => e.stopPropagation()}
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
        >
          <button
            type="button"
            className="context-menu__item"
            role="menuitem"
            onClick={() => {
              closeMenu();
              void openPetSettingsFromMenu();
            }}
          >
            {t("pet.menu.edit")}
          </button>
          <button
            type="button"
            className="context-menu__item"
            role="menuitem"
            onClick={() => {
              closeMenu();
              void petOpenSettings();
            }}
          >
            {t("pet.menu.settings")}
          </button>
          <div className="context-menu__sep" role="separator" />
          <button
            type="button"
            className="context-menu__item"
            role="menuitem"
            onClick={() => {
              closeMenu();
              void petHide();
            }}
          >
            {t("pet.menu.hide")}
          </button>
        </div>
      ) : null}
    </div>
  );
}
