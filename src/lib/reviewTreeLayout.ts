/**
 * Change-view file tree width. Separate from the Files pane width so
 * dragging one column does not move the other.
 */

export const REVIEW_TREE_WIDTH_KEY = "grok-app.reviewTreeWidth";
export const REVIEW_TREE_WIDTH_DEFAULT = 240;
export const REVIEW_TREE_WIDTH_MIN = 180;
export const REVIEW_TREE_WIDTH_MAX = 640;
/** Share of the review split the tree may take; the diff keeps the rest. */
export const REVIEW_TREE_WIDTH_FRACTION = 0.62;

function defaultStorage(): Storage | null {
  try {
    if (typeof localStorage !== "undefined") return localStorage;
  } catch {
    /* ignore */
  }
  return null;
}

export function loadReviewTreeWidth(
  storage: Pick<Storage, "getItem"> | null = defaultStorage(),
): number {
  try {
    const n = Number(storage?.getItem(REVIEW_TREE_WIDTH_KEY));
    if (
      Number.isFinite(n) &&
      n >= REVIEW_TREE_WIDTH_MIN &&
      n <= REVIEW_TREE_WIDTH_MAX
    ) {
      return Math.round(n);
    }
  } catch {
    /* ignore */
  }
  return REVIEW_TREE_WIDTH_DEFAULT;
}

export function clampReviewTreeWidth(w: number, containerWidth: number): number {
  const raw = Number.isFinite(w) ? Math.round(w) : REVIEW_TREE_WIDTH_DEFAULT;
  const bounded = Math.min(
    REVIEW_TREE_WIDTH_MAX,
    Math.max(REVIEW_TREE_WIDTH_MIN, raw),
  );
  if (!Number.isFinite(containerWidth) || containerWidth <= 0) return bounded;
  const maxByContainer = Math.max(
    REVIEW_TREE_WIDTH_MIN,
    Math.floor(containerWidth * REVIEW_TREE_WIDTH_FRACTION),
  );
  const max = Math.min(REVIEW_TREE_WIDTH_MAX, maxByContainer);
  return Math.min(max, bounded);
}

/** Clamp and persist the review tree width; returns the stored value. */
export function persistReviewTreeWidth(
  w: number,
  containerWidth: number,
  storage: Pick<Storage, "setItem"> | null = defaultStorage(),
): number {
  const next = clampReviewTreeWidth(w, containerWidth);
  try {
    storage?.setItem(REVIEW_TREE_WIDTH_KEY, String(next));
  } catch {
    /* ignore */
  }
  return next;
}
