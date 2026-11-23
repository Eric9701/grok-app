import { describe, expect, it } from "vitest";
import {
  commitFailureKind,
  commitPushMode,
  commitPushNeedsMessage,
  pushOutputIsUpToDate,
} from "./envCommitPush";

describe("envCommitPush", () => {
  it("requires a message only when files will be committed", () => {
    expect(commitPushNeedsMessage(2)).toBe(true);
    expect(commitPushNeedsMessage(0)).toBe(false);
    expect(commitPushMode(2)).toBe("commit-push");
    expect(commitPushMode(0)).toBe("push");
  });

  it("recognizes an already-published branch", () => {
    expect(pushOutputIsUpToDate("", "Everything up-to-date")).toBe(true);
    expect(pushOutputIsUpToDate("dev -> dev", "")).toBe(false);
  });

  it("maps host reason codes", () => {
    expect(commitFailureKind("detached_head")).toBe("detached");
    expect(commitFailureKind("message_required")).toBe("message");
    expect(commitFailureKind("Author identity unknown")).toBe("other");
  });
});
