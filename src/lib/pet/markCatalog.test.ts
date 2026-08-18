import { describe, expect, it } from "vitest";
import shapes from "@/lib/pet/data/shapes.json";
import eyes from "@/lib/pet/data/eyes.json";
import { STATE_TOPOLOGIES, verbToMarkState } from "@/lib/pet/markTables";
import { lerpPts, polyPath, spring, stepSpring } from "@/lib/pet/markMath";

describe("ported Sand mark catalog", () => {
  it("ships official Jo paths for picker shapes", () => {
    for (const id of ["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"]) {
      const rec = (shapes as Record<string, { path: string }>)[id];
      expect(rec?.path.startsWith("M"), id).toBe(true);
      expect(rec.path.length).toBeGreaterThan(40);
    }
  });

  it("ships 25 eye topologies of two 48-point eyes", () => {
    expect(eyes).toHaveLength(25);
    const pair = eyes[0] as number[][][];
    expect(pair).toHaveLength(2);
    expect(pair[0]).toHaveLength(48);
    expect(pair[0][0]).toHaveLength(2);
  });

  it("maps waiting to listening (original expectant pose)", () => {
    expect(verbToMarkState("waiting")).toBe("listening");
    expect(STATE_TOPOLOGIES.listening.length).toBeGreaterThan(0);
  });

  it("spring + lerp are the original stepping primitives", () => {
    const s = spring(0);
    s.t = 1;
    for (let i = 0; i < 40; i++) stepSpring(s, 10, 0.8, 1 / 60);
    expect(s.x).toBeGreaterThan(0.8);
    const out = lerpPts([[0, 0], [2, 2]], [[4, 4], [6, 6]], 0.5);
    expect(out[0]).toEqual([2, 2]);
    expect(polyPath([[1, 2], [3, 4]])).toBe("M1.00 2.00L3.00 4.00Z");
  });
});
