import { describe, expect, it } from "vitest";
import {
  ATLAS_CLI_MIRROR_INTRANET,
  ATLAS_CLI_MIRROR_PUBLIC,
  classifyAtlasCliMirror,
  formatAtlasCliInstallCommand,
  normalizeAtlasCliMirror,
  resolveAtlasCliMirror,
} from "./atlasCliMirror";

describe("resolveAtlasCliMirror", () => {
  it("maps the three setup choices", () => {
    expect(resolveAtlasCliMirror("intranet", "")).toBe(ATLAS_CLI_MIRROR_INTRANET);
    expect(resolveAtlasCliMirror("public", "ignored")).toBe(ATLAS_CLI_MIRROR_PUBLIC);
    expect(
      resolveAtlasCliMirror("custom", "https://mirror.example/atlas/cli/"),
    ).toBe("https://mirror.example/atlas/cli");
  });

  it("rejects empty, non-http, userinfo, and traversal", () => {
    expect(resolveAtlasCliMirror("custom", "  ")).toBeNull();
    expect(resolveAtlasCliMirror("custom", "ftp://mirror.example/atlas/cli")).toBeNull();
    expect(
      resolveAtlasCliMirror("custom", "http://user:pass@mirror.example/atlas/cli"),
    ).toBeNull();
    expect(
      resolveAtlasCliMirror("custom", "http://mirror.example/atlas/../cli"),
    ).toBeNull();
    expect(normalizeAtlasCliMirror("not a url")).toBeNull();
  });
});

describe("classifyAtlasCliMirror", () => {
  it("defaults empty and the intranet preset to MY内", () => {
    expect(classifyAtlasCliMirror("")).toEqual({ choice: "intranet", custom: "" });
    expect(classifyAtlasCliMirror(null)).toEqual({ choice: "intranet", custom: "" });
    expect(classifyAtlasCliMirror(`${ATLAS_CLI_MIRROR_INTRANET}/`)).toEqual({
      choice: "intranet",
      custom: "",
    });
  });

  it("selects MY外 or other", () => {
    expect(classifyAtlasCliMirror(ATLAS_CLI_MIRROR_PUBLIC)).toEqual({
      choice: "public",
      custom: "",
    });
    expect(classifyAtlasCliMirror("https://other.example/atlas/cli")).toEqual({
      choice: "custom",
      custom: "https://other.example/atlas/cli",
    });
  });
});

describe("formatAtlasCliInstallCommand", () => {
  it("uses install.ps1 on Windows and install.sh elsewhere", () => {
    const base = ATLAS_CLI_MIRROR_PUBLIC;
    expect(formatAtlasCliInstallCommand(base, "win")).toBe(
      `irm ${base}/install.ps1 | iex`,
    );
    expect(formatAtlasCliInstallCommand(base, "mac")).toBe(
      `curl -fsSL ${base}/install.sh | bash`,
    );
    expect(formatAtlasCliInstallCommand(base, "linux")).toBe(
      `curl -fsSL ${base}/install.sh | bash`,
    );
  });
});
