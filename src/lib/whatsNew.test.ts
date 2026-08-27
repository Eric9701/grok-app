import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  WHATS_NEW_FIRST_SEEN_KEY,
  WHATS_NEW_SEEN_KEY,
  changelogLang,
  ensureFirstSeenVersion,
  loadSeenVersion,
  markWhatsNewSeen,
  parseChangelogNotes,
  shouldAutoShowWhatsNew,
  type WhatsNewStorage,
} from "./whatsNew";

const FIXTURE = `# Changelog

## [Unreleased]

### Added
- **Dev only**: should not be picked for 1.2.3

**中文 · 新增**
- **仅开发**：不该出现在 1.2.3

## [1.2.3] - 2026-08-01

> **Highlight:** Fast cats.
>
> **中文 · 亮点：** 快猫。

### Added
- **Cat zoom**: Cats zoom now.

**中文 · 新增**
- **猫咪变焦**：猫咪会变焦了。

### Changed
- **Windows paths**: Strip prefixes.

**中文 · 变更**
- **Windows 路径**：去掉前缀。

### Fixed
- **Crash**: No more crash.

**中文 · 修复**
- **崩溃**：不再崩溃。

## [1.2.2] - 2026-07-01

### Added
- **Old**: leftover.

**中文 · 新增**
- **旧**：残留。
`;

function memoryStorage(
  initial: Record<string, string> = {},
): WhatsNewStorage & { data: Record<string, string> } {
  const data = { ...initial };
  return {
    data,
    getItem(key) {
      return key in data ? data[key]! : null;
    },
    setItem(key, value) {
      data[key] = value;
    },
    removeItem(key) {
      delete data[key];
    },
  };
}

describe("parseChangelogNotes", () => {
  it("extracts English highlight and sections for a version", () => {
    const notes = parseChangelogNotes(FIXTURE, "1.2.3", "en");
    expect(notes).not.toBeNull();
    expect(notes?.version).toBe("1.2.3");
    expect(notes?.date).toBe("2026-08-01");
    expect(notes?.highlight).toBe("Fast cats.");
    expect(notes?.sections.map((s) => s.id)).toEqual([
      "added",
      "changed",
      "fixed",
    ]);
    expect(notes?.sections[0]?.items).toEqual(["Cat zoom: Cats zoom now."]);
    expect(notes?.sections[1]?.items[0]).toContain("Windows paths");
    expect(notes?.sections[2]?.items[0]).toContain("Crash");
    expect(notes?.sections.flatMap((s) => s.items).join(" ")).not.toContain(
      "Dev only",
    );
  });

  it("extracts Chinese highlight and sections for zh catalogs", () => {
    const notes = parseChangelogNotes(FIXTURE, "1.2.3", "zh");
    expect(notes?.highlight).toBe("快猫。");
    expect(notes?.sections[0]?.items).toEqual(["猫咪变焦：猫咪会变焦了。"]);
    expect(notes?.sections[1]?.items[0]).toContain("Windows 路径");
    expect(notes?.sections[2]?.items[0]).toContain("崩溃");
  });

  it("returns null when the version section is missing", () => {
    expect(parseChangelogNotes(FIXTURE, "9.9.9", "en")).toBeNull();
  });
});

describe("changelogLang", () => {
  it("uses Chinese CHANGELOG blocks for zh and zh-TW only", () => {
    expect(changelogLang("zh")).toBe("zh");
    expect(changelogLang("zh-TW")).toBe("zh");
    expect(changelogLang("en")).toBe("en");
    expect(changelogLang("ja")).toBe("en");
    expect(changelogLang("de")).toBe("en");
  });
});

describe("whatsNew seen / auto-show", () => {
  it("round-trips seen version", () => {
    const s = memoryStorage();
    expect(loadSeenVersion(s)).toBeNull();
    markWhatsNewSeen("1.2.3", s);
    expect(s.data[WHATS_NEW_SEEN_KEY]).toBe("1.2.3");
    expect(loadSeenVersion(s)).toBe("1.2.3");
  });

  it("records first-seen version once", () => {
    const s = memoryStorage();
    expect(ensureFirstSeenVersion("1.2.3", s)).toBe("1.2.3");
    expect(s.data[WHATS_NEW_FIRST_SEEN_KEY]).toBe("1.2.3");
    expect(ensureFirstSeenVersion("1.2.4", s)).toBe("1.2.3");
  });

  it("does not auto-show during setup, tutorial, or before the gate", () => {
    expect(
      shouldAutoShowWhatsNew({
        currentVersion: "1.2.3",
        seenVersion: null,
        firstSeenVersion: "1.0.0",
        gateReady: false,
        setupOpen: false,
        tutorialOpen: false,
      }),
    ).toBe(false);
    expect(
      shouldAutoShowWhatsNew({
        currentVersion: "1.2.3",
        seenVersion: null,
        firstSeenVersion: "1.0.0",
        gateReady: true,
        setupOpen: true,
        tutorialOpen: false,
      }),
    ).toBe(false);
    expect(
      shouldAutoShowWhatsNew({
        currentVersion: "1.2.3",
        seenVersion: null,
        firstSeenVersion: "1.0.0",
        gateReady: true,
        setupOpen: false,
        tutorialOpen: true,
      }),
    ).toBe(false);
  });

  it("does not auto-show a fresh install of the current version", () => {
    expect(
      shouldAutoShowWhatsNew({
        currentVersion: "1.2.3",
        seenVersion: null,
        firstSeenVersion: "1.2.3",
        gateReady: true,
        setupOpen: false,
        tutorialOpen: false,
      }),
    ).toBe(false);
  });

  it("auto-shows an upgrade from a previous version", () => {
    expect(
      shouldAutoShowWhatsNew({
        currentVersion: "1.2.4",
        seenVersion: "1.2.3",
        firstSeenVersion: "1.2.3",
        gateReady: true,
        setupOpen: false,
        tutorialOpen: false,
      }),
    ).toBe(true);
  });

  it("auto-shows a legacy install that never recorded first-seen", () => {
    expect(
      shouldAutoShowWhatsNew({
        currentVersion: "1.2.3",
        seenVersion: null,
        firstSeenVersion: null,
        gateReady: true,
        setupOpen: false,
        tutorialOpen: false,
      }),
    ).toBe(true);
  });

  it("does not auto-show the same version twice", () => {
    expect(
      shouldAutoShowWhatsNew({
        currentVersion: "1.2.3",
        seenVersion: "1.2.3",
        firstSeenVersion: "1.0.0",
        gateReady: true,
        setupOpen: false,
        tutorialOpen: false,
      }),
    ).toBe(false);
  });
});

describe("shipped CHANGELOG.md", () => {
  it("has a section for the package version or Unreleased notes", () => {
    const md = readFileSync(resolve(__dirname, "../../CHANGELOG.md"), "utf8");
    const pkg = JSON.parse(
      readFileSync(resolve(__dirname, "../../package.json"), "utf8"),
    ) as { version: string };
    const notes =
      parseChangelogNotes(md, pkg.version, "en") ??
      parseChangelogNotes(md, "Unreleased", "en");
    expect(notes).not.toBeNull();
    expect((notes?.sections.length ?? 0) + (notes?.highlight ? 1 : 0)).toBeGreaterThan(
      0,
    );
  });
});
