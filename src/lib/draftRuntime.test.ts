import { afterEach, describe, expect, it } from "vitest";
import {
  clearDraftRuntimeOverride,
  draftRuntimeOverride,
  resolveDraftProfileId,
  setDraftRuntimeOverride,
} from "./draftRuntime";

afterEach(() => {
  clearDraftRuntimeOverride();
});

describe("resolveDraftProfileId", () => {
  it("keeps the desktop default until the user picks", () => {
    expect(resolveDraftProfileId(undefined, "claude-1")).toBe("claude-1");
    expect(resolveDraftProfileId(undefined, "")).toBe("");
  });

  it("treats an explicit empty pick as Current Atlas", () => {
    expect(resolveDraftProfileId("", "claude-1")).toBe("");
  });

  it("uses the profile the user picked on the new chat", () => {
    expect(resolveDraftProfileId("cursor-1", "claude-1")).toBe("cursor-1");
  });
});

describe("draftRuntimeOverride", () => {
  it("clears back to untouched", () => {
    setDraftRuntimeOverride("cursor-1");
    expect(draftRuntimeOverride()).toBe("cursor-1");
    clearDraftRuntimeOverride();
    expect(draftRuntimeOverride()).toBeUndefined();
  });
});
