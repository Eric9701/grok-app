/**
 * Codex-style task bubbles for the desktop pet.
 *
 * Only real work: streaming/busy (`working`) and unread finished turns
 * (`ready`). A done chip stays until the user clicks it or actually reads
 * that chat with the main window focused. Connecting / permission / error /
 * idle never get a chip — switching chats must not flash a "connecting"
 * bubble.
 */

import {
  kindForSession,
  petKindRank,
  type PetFocusInput,
  type PetKind,
} from "./petFocus";

/** How many chips are visible at once. Extra rows scroll inside the slot. */
export const PET_BUBBLE_VISIBLE = 3;
/** Collect cap — more than the viewport so the slot can scroll. */
export const PET_TASK_LIMIT = 16;
export const PET_BUBBLE_WIDTH = 216;
export const PET_BUBBLE_ROW_H = 38;
export const PET_BUBBLE_GAP = 6;
export const PET_BUBBLE_STACK_PAD = 10;
/** Inner padding so chip drop-shadows are not square-clipped by overflow. */
export const PET_BUBBLE_SHADOW_PAD = 20;

export type PetTaskPhase = "active" | "done";

export type PetTask = {
  sessionId: string;
  title: string | null;
  toolTitle: string | null;
  kind: PetKind;
  phase: PetTaskPhase;
  /** 0..1 phase progress. Active chips animate; done is 1. */
  progress: number;
  updatedAt: number;
};

/** Bubbles are only in-progress work or completed unread. */
export function isPetTaskBubbleKind(kind: PetKind): boolean {
  return kind === "working" || kind === "ready";
}

export function petTaskPhase(kind: PetKind): PetTaskPhase {
  return kind === "ready" ? "done" : "active";
}

export function petTaskProgress(kind: PetKind): number {
  switch (kind) {
    case "ready":
    case "error":
      return 1;
    case "needs_you":
      return 0.72;
    case "working":
      return 0.55;
    case "connecting":
      return 0.22;
    default:
      return 0;
  }
}

export function petBubbleStackHeight(count: number): number {
  const n = Math.max(0, Math.min(PET_BUBBLE_VISIBLE, Math.floor(count)));
  if (n <= 0) return 0;
  return n * PET_BUBBLE_ROW_H + (n - 1) * PET_BUBBLE_GAP + PET_BUBBLE_STACK_PAD;
}

/** Reserved slot above the mark. Always this tall so the pet never jumps. */
export function petBubbleViewportHeight(): number {
  return petBubbleStackHeight(PET_BUBBLE_VISIBLE) + PET_BUBBLE_SHADOW_PAD * 2;
}

function collectIds(input: PetFocusInput): string[] {
  const ids = new Set<string>();
  for (const id of Object.keys(input.liveMap)) {
    if (id) ids.add(id);
  }
  for (const id of input.unreadIds) {
    if (id) ids.add(id);
  }
  for (const id of Object.keys(input.finishedTurns)) {
    if (id) ids.add(id);
  }
  for (const s of input.sessions) {
    if (s.id) ids.add(s.id);
  }
  return [...ids];
}

function titleFor(sessionId: string, input: PetFocusInput): string | null {
  const row = input.sessions.find((s) => s.id === sessionId);
  const t = row?.title?.trim();
  return t ? t : null;
}

function activityAt(
  sessionId: string,
  kind: PetKind,
  input: PetFocusInput,
): number {
  const snap = input.liveMap[sessionId];
  if (kind === "ready") {
    return input.finishedTurns[sessionId] ?? snap?.updatedAt ?? 0;
  }
  return snap?.startedAt ?? snap?.updatedAt ?? 0;
}

/** Working + unread-complete sessions, highest-priority first, capped. */
export function collectPetTasks(input: PetFocusInput): PetTask[] {
  const rows: PetTask[] = [];
  for (const id of collectIds(input)) {
    const kind = kindForSession(id, input);
    if (!isPetTaskBubbleKind(kind)) continue;
    const snap = input.liveMap[id];
    rows.push({
      sessionId: id,
      title: titleFor(id, input),
      toolTitle: snap?.liveToolTitle ?? null,
      kind,
      phase: petTaskPhase(kind),
      progress: petTaskProgress(kind),
      updatedAt: activityAt(id, kind, input),
    });
  }
  rows.sort((a, b) => {
    const rank = petKindRank(a.kind) - petKindRank(b.kind);
    if (rank !== 0) return rank;
    if (b.updatedAt !== a.updatedAt) return b.updatedAt - a.updatedAt;
    return a.sessionId < b.sessionId ? -1 : 1;
  });
  return rows.slice(0, PET_TASK_LIMIT);
}

export function samePetTasks(
  a: readonly PetTask[],
  b: readonly PetTask[],
): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    const x = a[i]!;
    const y = b[i]!;
    if (
      x.sessionId !== y.sessionId ||
      x.kind !== y.kind ||
      x.phase !== y.phase ||
      x.title !== y.title ||
      x.toolTitle !== y.toolTitle
    ) {
      return false;
    }
  }
  return true;
}
