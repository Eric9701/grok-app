import { describe, expect, it } from "vitest";
import {
  hideSshProjectInLocalTree,
  isProjectPathMissing,
  isProjectWarmable,
  isSshRemoteProject,
} from "./projectPath";

describe("isProjectPathMissing", () => {
  it("is true only for explicit false", () => {
    expect(isProjectPathMissing(false)).toBe(true);
  });

  it("treats true / nullish as ok (do not invent missing)", () => {
    expect(isProjectPathMissing(true)).toBe(false);
    expect(isProjectPathMissing(undefined)).toBe(false);
    expect(isProjectPathMissing(null)).toBe(false);
  });
});

describe("isSshRemoteProject", () => {
  it("is true only when sshAlias is a non-empty string", () => {
    expect(isSshRemoteProject({ sshAlias: "uts" })).toBe(true);
    expect(isSshRemoteProject({ sshAlias: "  gw-01  " })).toBe(true);
    expect(isSshRemoteProject({ sshAlias: "" })).toBe(false);
    expect(isSshRemoteProject({ sshAlias: "   " })).toBe(false);
    expect(isSshRemoteProject({ sshAlias: null })).toBe(false);
    expect(isSshRemoteProject({})).toBe(false);
    expect(isSshRemoteProject(null)).toBe(false);
  });
});

describe("isProjectWarmable", () => {
  it("allows orphan chats and trusted local folders", () => {
    expect(isProjectWarmable(null)).toBe(true);
    expect(isProjectWarmable({ trusted: true, pathOk: true })).toBe(true);
  });

  it("blocks untrusted or missing local paths", () => {
    expect(isProjectWarmable({ trusted: false, pathOk: true })).toBe(false);
    expect(isProjectWarmable({ trusted: true, pathOk: false })).toBe(false);
  });

  it("never warms an SSH remote even when pathOk is true", () => {
    expect(
      isProjectWarmable({
        trusted: true,
        pathOk: true,
        sshAlias: "uts",
      }),
    ).toBe(false);
  });
});

describe("hideSshProjectInLocalTree", () => {
  it("hides an SSH project only while that host is watching", () => {
    expect(
      hideSshProjectInLocalTree({ sshAlias: "UTS" }, ["UTS"]),
    ).toBe(true);
    expect(
      hideSshProjectInLocalTree({ sshAlias: "UTS" }, ["other"]),
    ).toBe(false);
    expect(hideSshProjectInLocalTree({ sshAlias: "UTS" }, [])).toBe(false);
    expect(hideSshProjectInLocalTree({}, ["UTS"])).toBe(false);
  });
});
