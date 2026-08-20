import { describe, expect, it } from "vitest";
import { BotEngine, POSES } from "./bloub";
import {
  PET_COMPOSING_HOLD_MS,
  bloubExpressionOf,
  bloubNotifFill,
  bloubShapeId,
  bloubShapeRadii,
  normalizePetExpression,
  petIsComposing,
  petVerbForComposer,
  resolveBloubPlay,
} from "./bloubPlay";

describe("bloub product mapping", () => {
  it("maps saved pet shapes onto the 8 bloub skins", () => {
    expect(bloubShapeId("hex")).toBe("hexagone");
    expect(bloubShapeId("blob")).toBe("cercle");
    expect(bloubShapeId("wedge")).toBe("triangle");
    expect(bloubShapeId("cloud")).toBe("nuage");
    expect(bloubShapeId("leaf")).toBe("goutte");
    expect(bloubShapeId("nope")).toBe("cercle");
  });

  it("maps session verbs onto measured animation states", () => {
    expect(resolveBloubPlay("writing", "neutre").state).toBe("hexagon");
    expect(resolveBloubPlay("notifying", "neutre").state).toBe("notify");
    expect(resolveBloubPlay("waiting", "neutre").state).toBe("alert");
    expect(resolveBloubPlay("thinking", "neutre").state).toBe("thinking");
    expect(resolveBloubPlay("searching", "neutre").state).toBe("comet");
    expect(resolveBloubPlay("working", "neutre").state).toBe("orbit");
    expect(resolveBloubPlay("sad", "neutre").state).toBe("exclaim");
    expect(resolveBloubPlay("celebrate", "neutre").state).toBe("idle");
  });

  it("maps rest moods onto idle + expression", () => {
    const play = resolveBloubPlay("laughing", "neutre");
    expect(play.state).toBe("idle");
    expect(play.expression).toBe("hilare");
    expect(resolveBloubPlay("idle", "curieux").expression).toBe("curieux");
    expect(normalizePetExpression("nope")).toBe("neutre");
  });

  it("turns composer typing into the catalog hexagon morph while idle", () => {
    expect(
      petVerbForComposer({ sessionVerb: "idle", composing: true }),
    ).toBe("writing");
    expect(
      petVerbForComposer({ sessionVerb: "working", composing: true }),
    ).toBe("working");
    expect(
      petVerbForComposer({ sessionVerb: "idle", composing: false }),
    ).toBe("idle");
  });

  it("drops the hexagon morph when typing pauses or the draft is empty", () => {
    expect(
      petIsComposing({ empty: true, lastTypeAt: 1000, now: 1100 }),
    ).toBe(false);
    expect(
      petIsComposing({ empty: false, lastTypeAt: 0, now: 5000 }),
    ).toBe(false);
    expect(
      petIsComposing({ empty: false, lastTypeAt: 1000, now: 1100 }),
    ).toBe(true);
    expect(
      petIsComposing({
        empty: false,
        lastTypeAt: 1000,
        now: 1000 + PET_COMPOSING_HOLD_MS + 1,
      }),
    ).toBe(false);
  });

  it("uses a non-blue unread pastille", () => {
    expect(bloubNotifFill("#111111")).toBe("#FF3B1A");
    expect(bloubNotifFill("#FF3B1A")).toBe("#C8FF00");
  });

  it("samples idle, hexagon, and notify frames from the engine", () => {
    const engine = new BotEngine(
      100,
      "idle",
      bloubShapeRadii("blob"),
      bloubExpressionOf("neutre"),
    );
    const idle = engine.sample(POSES.idle);
    expect(idle.bodyPath.startsWith("M")).toBe(true);
    expect(idle.eyes.length).toBe(2);
    engine.setState("hexagon", 0);
    const hex = engine.sample(POSES.hexagon);
    expect(hex.bodyPath.startsWith("M")).toBe(true);
    expect(hex.bodyPath).not.toBe(idle.bodyPath);
    engine.reset("notify", 0);
    const note = engine.sample(POSES.notify);
    expect(note.notif).not.toBeNull();
    expect(note.notif!.r).toBeGreaterThan(0);
  });
});
