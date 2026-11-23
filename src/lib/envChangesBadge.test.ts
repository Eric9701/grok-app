import { describe, expect, it } from "vitest";
import { envChangesBadge } from "./envChangesBadge";

describe("envChangesBadge", () => {
  it("keeps the session file count until git status is known", () => {
    expect(
      envChangesBadge({
        gitLoaded: false,
        isGit: true,
        dirtyCount: 0,
        sessionFileCount: 2,
      }),
    ).toEqual({ kind: "files", count: 2 });
  });

  it("drops the session count when the worktree is clean", () => {
    expect(
      envChangesBadge({
        gitLoaded: true,
        isGit: true,
        dirtyCount: 0,
        sessionAdd: 4,
        sessionDel: 1,
        sessionFileCount: 2,
      }),
    ).toEqual({ kind: "none" });
  });

  it("shows uncommitted git files instead of the session list", () => {
    expect(
      envChangesBadge({
        gitLoaded: true,
        isGit: true,
        dirtyCount: 1,
        sessionFileCount: 2,
      }),
    ).toEqual({ kind: "files", count: 1 });
  });

  it("uses the session summary when the folder is not a git repo", () => {
    expect(
      envChangesBadge({
        gitLoaded: true,
        isGit: false,
        dirtyCount: 0,
        sessionAdd: 3,
        sessionDel: 1,
      }),
    ).toEqual({ kind: "delta", add: 3, del: 1 });
  });
});
