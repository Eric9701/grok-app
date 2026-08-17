import { describe, expect, it, vi } from "vitest";
import {
  MIXED_DISPLAY_FRAME_FALLBACK_MS,
  cancelFrameSchedule,
  emptyFrameSchedule,
  isFrameSchedulePending,
  scheduleOnFrame,
  type FrameScheduleHost,
} from "./frameSchedule";

function mockHost() {
  let rafId = 0;
  let toId = 0;
  const rafs = new Map<number, FrameRequestCallback>();
  const tos = new Map<number, () => void>();
  const host: FrameScheduleHost = {
    raf: (cb) => {
      const id = ++rafId;
      rafs.set(id, cb);
      return id;
    },
    cancelRaf: (id) => {
      rafs.delete(id);
    },
    timeout: (cb, _ms) => {
      const id = ++toId;
      tos.set(id, cb);
      return id as unknown as ReturnType<typeof setTimeout>;
    },
    clearTimeout: (id) => {
      tos.delete(id as unknown as number);
    },
  };
  return {
    host,
    flushRaf: () => {
      const first = rafs.entries().next().value;
      if (!first) return;
      const [id, cb] = first;
      rafs.delete(id);
      cb(0);
    },
    flushTimeout: () => {
      const first = tos.entries().next().value;
      if (!first) return;
      const [id, cb] = first;
      tos.delete(id);
      cb();
    },
    rafCount: () => rafs.size,
    timeoutCount: () => tos.size,
  };
}

describe("scheduleOnFrame", () => {
  it("uses an 8ms fallback so a missed 75Hz vsync still commits", () => {
    expect(MIXED_DISPLAY_FRAME_FALLBACK_MS).toBe(8);
    expect(MIXED_DISPLAY_FRAME_FALLBACK_MS).toBeLessThan(1000 / 75);
  });

  it("runs once when rAF wins, and cancels the timeout", () => {
    const m = mockHost();
    const state = emptyFrameSchedule();
    const run = vi.fn();
    scheduleOnFrame(state, run, m.host);
    expect(isFrameSchedulePending(state)).toBe(true);
    scheduleOnFrame(state, run, m.host);
    expect(m.rafCount()).toBe(1);
    m.flushRaf();
    expect(run).toHaveBeenCalledTimes(1);
    expect(isFrameSchedulePending(state)).toBe(false);
    expect(m.timeoutCount()).toBe(0);
  });

  it("runs once when the timeout wins (missed vsync)", () => {
    const m = mockHost();
    const state = emptyFrameSchedule();
    const run = vi.fn();
    scheduleOnFrame(state, run, m.host);
    m.flushTimeout();
    expect(run).toHaveBeenCalledTimes(1);
    expect(isFrameSchedulePending(state)).toBe(false);
    expect(m.rafCount()).toBe(0);
  });

  it("cancelFrameSchedule drops both handles", () => {
    const m = mockHost();
    const state = emptyFrameSchedule();
    const run = vi.fn();
    scheduleOnFrame(state, run, m.host);
    cancelFrameSchedule(state, m.host);
    expect(isFrameSchedulePending(state)).toBe(false);
    expect(run).not.toHaveBeenCalled();
  });
});
