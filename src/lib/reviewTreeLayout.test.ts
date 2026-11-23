import { describe, expect, it } from "vitest";
import {
  REVIEW_TREE_WIDTH_DEFAULT,
  REVIEW_TREE_WIDTH_KEY,
  REVIEW_TREE_WIDTH_MAX,
  REVIEW_TREE_WIDTH_MIN,
  clampReviewTreeWidth,
  loadReviewTreeWidth,
  persistReviewTreeWidth,
} from "./reviewTreeLayout";

function memStorage(seed: Record<string, string> = {}): Storage {
  const map = new Map(Object.entries(seed));
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (k: string) => (map.has(k) ? map.get(k)! : null),
    key: (i: number) => [...map.keys()][i] ?? null,
    removeItem: (k: string) => {
      map.delete(k);
    },
    setItem: (k: string, v: string) => {
      map.set(k, v);
    },
  };
}

describe("review tree width", () => {
  it("clamps to min, max, and the split fraction", () => {
    expect(clampReviewTreeWidth(50, 1000)).toBe(REVIEW_TREE_WIDTH_MIN);
    expect(clampReviewTreeWidth(9999, 2000)).toBe(REVIEW_TREE_WIDTH_MAX);
    expect(clampReviewTreeWidth(9999, 800)).toBe(Math.floor(800 * 0.62));
    expect(clampReviewTreeWidth(320, 1000)).toBe(320);
    expect(clampReviewTreeWidth(Number.NaN, 1000)).toBe(
      REVIEW_TREE_WIDTH_DEFAULT,
    );
  });

  it("ignores an unmeasured container instead of collapsing the column", () => {
    expect(clampReviewTreeWidth(400, 0)).toBe(400);
  });

  it("persists a rounded width under its own key", () => {
    const s = memStorage();
    const w = persistReviewTreeWidth(359.4, 1000, s);
    expect(w).toBe(359);
    expect(s.getItem(REVIEW_TREE_WIDTH_KEY)).toBe("359");
    expect(loadReviewTreeWidth(s)).toBe(359);
  });

  it("falls back when the stored width is garbage", () => {
    const s = memStorage({ [REVIEW_TREE_WIDTH_KEY]: "nope" });
    expect(loadReviewTreeWidth(s)).toBe(REVIEW_TREE_WIDTH_DEFAULT);
  });
});
