import { describe, expect, it } from "vitest";
import {
  PET_PAINT_IDLE_MS,
  PET_PAINT_LIVE_MS,
  PET_PAINT_REST_AFTER_MS,
  PET_PAINT_REST_MS,
  PET_PAINT_SPIN_MS,
  petLookIsNear,
  petPaintMinMs,
} from "./petMarkPaint";

describe("petPaintMinMs", () => {
  it("keeps celebrate spin at full cadence", () => {
    expect(
      petPaintMinMs({
        spinning: true,
        morphing: false,
        trackingLook: false,
        idleMs: 60_000,
      }),
    ).toBe(PET_PAINT_SPIN_MS);
  });

  it("raises the floor while morphing or looking at the pointer", () => {
    expect(
      petPaintMinMs({
        spinning: false,
        morphing: true,
        trackingLook: false,
        idleMs: 0,
      }),
    ).toBe(PET_PAINT_LIVE_MS);
    expect(
      petPaintMinMs({
        spinning: false,
        morphing: false,
        trackingLook: true,
        idleMs: 0,
      }),
    ).toBe(PET_PAINT_LIVE_MS);
  });

  it("drops to a rest cadence after a long idle", () => {
    expect(
      petPaintMinMs({
        spinning: false,
        morphing: false,
        trackingLook: false,
        idleMs: 0,
      }),
    ).toBe(PET_PAINT_IDLE_MS);
    expect(
      petPaintMinMs({
        spinning: false,
        morphing: false,
        trackingLook: false,
        idleMs: PET_PAINT_REST_AFTER_MS,
      }),
    ).toBe(PET_PAINT_REST_MS);
  });
});

describe("petLookIsNear", () => {
  it("keeps a moving screen-space cursor near the mark", () => {
    expect(
      petLookIsNear({
        fromScreen: true,
        at: 9_500,
        now: 10_000,
        dx: 8,
        dy: 4,
        localR: 64,
      }),
    ).toBe(true);
    expect(
      petLookIsNear({
        fromScreen: true,
        at: 9_500,
        now: 10_000,
        dx: 200,
        dy: 200,
        localR: 64,
      }),
    ).toBe(false);
  });

  it("expires a parked screen-space cursor so idle paint tiers resume", () => {
    expect(
      petLookIsNear({
        fromScreen: true,
        at: 1,
        now: 10_000,
        dx: 8,
        dy: 4,
        localR: 64,
      }),
    ).toBe(false);
    expect(
      petLookIsNear({
        fromScreen: true,
        at: 9_500,
        now: 10_000,
        dx: 8,
        dy: 4,
        localR: 64,
      }),
    ).toBe(true);
  });

  it("expires overlay pointer events after a short hold", () => {
    expect(
      petLookIsNear({
        fromScreen: false,
        at: 1000,
        now: 1100,
        dx: 0,
        dy: 0,
        localR: 64,
      }),
    ).toBe(true);
    expect(
      petLookIsNear({
        fromScreen: false,
        at: 1000,
        now: 2000,
        dx: 0,
        dy: 0,
        localR: 64,
      }),
    ).toBe(false);
  });
});
