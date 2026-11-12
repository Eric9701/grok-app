import { describe, expect, it } from "vitest";
import { featureBit } from "./featureMask";

describe("featureBit", () => {
  it("reads bits from the right", () => {
    expect(featureBit(3, 0)).toBe(true);
    expect(featureBit(3, 1)).toBe(true);
    expect(featureBit(3, 2)).toBe(false);
    expect(featureBit(4, 2)).toBe(true);
    expect(featureBit(8, 3)).toBe(true);
    expect(featureBit(8, 2)).toBe(false);
    expect(featureBit(0, 0)).toBe(false);
  });

  it("rejects a mask outside 16 bits", () => {
    expect(featureBit(-1, 0)).toBe(false);
    expect(featureBit(65536, 0)).toBe(false);
    expect(featureBit(1.5, 0)).toBe(false);
  });
});
