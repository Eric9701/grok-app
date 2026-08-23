/**
 * High-precision scroll anchoring pixel compensation.
 *
 * When rows above the current viewport are measured and commit a height change
 * (actual - estimated = delta), mutating paddingTop would cause the viewport
 * content to jump by that delta.
 *
 * By synchronously compensating `viewport.scrollTop += delta` in the same
 * execution microtask, the visible content on screen maintains 0.000px displacement.
 */

export type HeightAdjustment = {
  prevHeight: number;
  nextHeight: number;
};

export function computeScrollAnchorAdjustment(input: {
  adjustments: Map<number, HeightAdjustment>;
  anchorIndex: number;
}): number {
  let totalDeltaAbove = 0;
  for (const [idx, adj] of input.adjustments) {
    if (idx < input.anchorIndex) {
      totalDeltaAbove += (adj.nextHeight - adj.prevHeight);
    }
  }
  return totalDeltaAbove;
}
