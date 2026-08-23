import { describe, it, expect } from "vitest";
import { computeScrollAnchorAdjustment } from "./chatScrollAnchor";

describe("computeScrollAnchorAdjustment", () => {
  it("calculates exact scrollTop delta when heights above viewport change", () => {
    const adjustments = new Map([[0, { prevHeight: 100, nextHeight: 140 }]]);
    const delta = computeScrollAnchorAdjustment({
      adjustments,
      anchorIndex: 1,
    });
    expect(delta).toBe(40);
  });

  it("ignores height changes at or below the anchor index", () => {
    const adjustments = new Map([
      [1, { prevHeight: 100, nextHeight: 150 }],
      [2, { prevHeight: 100, nextHeight: 200 }],
    ]);
    const delta = computeScrollAnchorAdjustment({
      adjustments,
      anchorIndex: 1,
    });
    expect(delta).toBe(0);
  });

  it("aggregates multiple adjustments above the anchor correctly", () => {
    const adjustments = new Map([
      [0, { prevHeight: 80, nextHeight: 120 }], // +40
      [2, { prevHeight: 200, nextHeight: 170 }], // -30
      [5, { prevHeight: 100, nextHeight: 300 }], // below anchor=4, ignored
    ]);
    const delta = computeScrollAnchorAdjustment({
      adjustments,
      anchorIndex: 4,
    });
    expect(delta).toBe(10);
  });
});
