import { describe, expect, it } from "vitest";
import { emptyLiveSnapshot, type SessionLiveMap } from "@/lib/sessionLiveStore";
import type { PetFocusInput } from "./petFocus";
import {
  collectPetTasks,
  PET_BUBBLE_SHADOW_PAD,
  PET_BUBBLE_VISIBLE,
  PET_TASK_LIMIT,
  petBubbleStackHeight,
  petBubbleViewportHeight,
  petTaskPhase,
  petTaskProgress,
  samePetTasks,
} from "./petTasks";

function snap(
  id: string,
  patch: Partial<SessionLiveMap[string]> & { state: SessionLiveMap[string]["state"] },
): SessionLiveMap[string] {
  return {
    ...emptyLiveSnapshot(id, patch.updatedAt ?? 1_000),
    ...patch,
    sessionId: id,
    awaitingPermission:
      patch.awaitingPermission ?? patch.state === "awaiting_permission",
  };
}

function input(partial: Partial<PetFocusInput> & { liveMap: SessionLiveMap }): PetFocusInput {
  return {
    unreadIds: new Set(),
    finishedTurns: {},
    sessions: Object.keys(partial.liveMap).map((id) => ({ id, title: id })),
    now: 10_000,
    ...partial,
  };
}

describe("collectPetTasks", () => {
  it("emits a bubble only for truly working sessions", () => {
    const liveMap: SessionLiveMap = {
      a: snap("a", { state: "streaming", liveToolTitle: "npm test", updatedAt: 9_000 }),
      b: snap("b", { state: "connecting", updatedAt: 8_000 }),
    };
    const tasks = collectPetTasks(input({ liveMap }));
    expect(tasks.map((t) => t.sessionId)).toEqual(["a"]);
    expect(tasks[0]?.phase).toBe("active");
    expect(tasks[0]?.progress).toBeGreaterThan(0);
    expect(tasks[0]?.progress).toBeLessThan(1);
  });

  it("omits connecting sessions when switching chats", () => {
    const liveMap: SessionLiveMap = {
      next: snap("next", { state: "connecting", updatedAt: 9_000 }),
    };
    expect(collectPetTasks(input({ liveMap }))).toEqual([]);
  });

  it("marks unread finished turns as completed", () => {
    const liveMap: SessionLiveMap = {
      done: snap("done", { state: "idle", updatedAt: 2_000 }),
    };
    const tasks = collectPetTasks(
      input({
        liveMap,
        unreadIds: new Set(["done"]),
        finishedTurns: { done: 8_000 },
        sessions: [{ id: "done", title: "Ship pet" }],
      }),
    );
    expect(tasks).toHaveLength(1);
    expect(tasks[0]?.phase).toBe("done");
    expect(tasks[0]?.progress).toBe(1);
    expect(tasks[0]?.title).toBe("Ship pet");
  });

  it("omits idle / read sessions", () => {
    const liveMap: SessionLiveMap = {
      idle: snap("idle", { state: "idle", updatedAt: 2_000 }),
    };
    expect(collectPetTasks(input({ liveMap }))).toEqual([]);
  });

  it("keeps a completed bubble until unread is cleared", () => {
    const liveMap: SessionLiveMap = {
      done: snap("done", { state: "idle", updatedAt: 2_000 }),
    };
    const unread = collectPetTasks(
      input({
        liveMap,
        unreadIds: new Set(["done"]),
        finishedTurns: { done: 8_000 },
      }),
    );
    expect(unread).toHaveLength(1);
    expect(unread[0]?.phase).toBe("done");
    expect(
      collectPetTasks(
        input({
          liveMap,
          unreadIds: new Set(),
          finishedTurns: { done: 8_000 },
        }),
      ),
    ).toEqual([]);
  });

  it("collects more than the visible viewport and prefers unread-complete", () => {
    const liveMap: SessionLiveMap = {
      p: snap("p", { state: "awaiting_permission", updatedAt: 1 }),
      e: snap("e", { state: "disconnected", updatedAt: 2 }),
      c: snap("c", { state: "connecting", updatedAt: 8 }),
      w1: snap("w1", { state: "streaming", updatedAt: 5 }),
      w2: snap("w2", { state: "streaming", updatedAt: 4 }),
      w3: snap("w3", { state: "streaming", updatedAt: 3 }),
      w4: snap("w4", { state: "streaming", updatedAt: 2 }),
    };
    const tasks = collectPetTasks(
      input({
        liveMap,
        unreadIds: new Set(["d"]),
        finishedTurns: { d: 9 },
        sessions: [
          { id: "p", title: "p" },
          { id: "e", title: "e" },
          { id: "c", title: "c" },
          { id: "w1", title: "w1" },
          { id: "w2", title: "w2" },
          { id: "w3", title: "w3" },
          { id: "w4", title: "w4" },
          { id: "d", title: "d" },
        ],
      }),
    );
    expect(tasks.length).toBeGreaterThan(PET_BUBBLE_VISIBLE);
    expect(tasks).toHaveLength(5);
    expect(tasks.map((t) => t.kind)).toEqual([
      "ready",
      "working",
      "working",
      "working",
      "working",
    ]);
    expect(tasks.some((t) => t.kind === "connecting")).toBe(false);
    expect(tasks.some((t) => t.kind === "needs_you")).toBe(false);
    expect(tasks.some((t) => t.kind === "error")).toBe(false);
  });

  it("caps the collected list well above the visible viewport", () => {
    const liveMap: SessionLiveMap = {};
    const sessions: { id: string; title: string }[] = [];
    for (let i = 0; i < PET_TASK_LIMIT + 4; i++) {
      const id = `w${i}`;
      liveMap[id] = snap(id, { state: "streaming", updatedAt: 100 - i });
      sessions.push({ id, title: id });
    }
    const tasks = collectPetTasks(input({ liveMap, sessions }));
    expect(tasks).toHaveLength(PET_TASK_LIMIT);
  });
});

describe("pet task helpers", () => {
  it("maps ready to done and working to active", () => {
    expect(petTaskPhase("ready")).toBe("done");
    expect(petTaskPhase("working")).toBe("active");
    expect(petTaskProgress("ready")).toBe(1);
    expect(petTaskProgress("working")).toBeLessThan(1);
  });

  it("sizes the visible viewport at 3 rows and ignores extras", () => {
    expect(petBubbleStackHeight(0)).toBe(0);
    expect(petBubbleStackHeight(1)).toBeGreaterThan(0);
    expect(petBubbleStackHeight(3)).toBeGreaterThan(petBubbleStackHeight(1));
    expect(petBubbleStackHeight(9)).toBe(petBubbleStackHeight(3));
    expect(petBubbleViewportHeight()).toBe(
      petBubbleStackHeight(PET_BUBBLE_VISIBLE) + PET_BUBBLE_SHADOW_PAD * 2,
    );
  });

  it("samePetTasks ignores progress jitter", () => {
    const a = collectPetTasks(
      input({
        liveMap: { a: snap("a", { state: "streaming", updatedAt: 1 }) },
      }),
    );
    const b = a.map((t) => ({ ...t, progress: 0.9, updatedAt: 99 }));
    expect(samePetTasks(a, b)).toBe(true);
    expect(samePetTasks(a, [])).toBe(false);
  });
});
