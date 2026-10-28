import { describe, expect, it } from "vitest";
import {
  feedHost,
  selfUpdateBlockerKey,
  selfUpdateErrorKey,
  selfUpdatePercent,
  shouldAutoPromptReady,
} from "./selfUpdate";

describe("selfUpdate helpers", () => {
  it("maps Host error strings to localized keys", () => {
    expect(selfUpdateErrorKey("invalid_mirror")).toBe("settings.selfUpdate.error.invalidMirror");
    expect(selfUpdateErrorKey("feed_not_published")).toBe("settings.selfUpdate.error.notPublished");
    expect(selfUpdateErrorKey("disk_space")).toBe("settings.selfUpdate.error.diskSpace");
    expect(selfUpdateErrorKey("busy")).toBe("settings.selfUpdate.error.busy");
    expect(selfUpdateErrorKey("checksum: size mismatch")).toBe("settings.selfUpdate.error.checksum");
    expect(selfUpdateErrorKey("network: timed out")).toBe("settings.autoUpdateError.network");
    expect(selfUpdateErrorKey("not_writable")).toBe("settings.selfUpdate.blocker.notWritable");
    expect(selfUpdateErrorKey("something else")).toBeNull();
    expect(selfUpdateErrorKey("")).toBeNull();
  });

  it("maps every Host blocker", () => {
    for (const b of [
      "debug_build",
      "not_installed_copy",
      "translocated",
      "not_writable",
      "unsupported_platform",
      "no_payload",
    ]) {
      expect(selfUpdateBlockerKey(b)).toMatch(/^settings\.selfUpdate\.blocker\./);
    }
    expect(selfUpdateBlockerKey(null)).toBeNull();
    expect(selfUpdateBlockerKey("unknown")).toBeNull();
  });

  it("computes download percent", () => {
    expect(selfUpdatePercent(null)).toBeNull();
    expect(selfUpdatePercent({ version: "1", downloaded: 5, total: 0 })).toBeNull();
    expect(selfUpdatePercent({ version: "1", downloaded: 50, total: 200 })).toBe(25);
    expect(selfUpdatePercent({ version: "1", downloaded: 300, total: 200 })).toBe(100);
  });

  it("reads the feed host", () => {
    expect(feedHost("https://cli.example.test/atlas/desktop/stable/update.json")).toBe(
      "cli.example.test",
    );
    expect(feedHost("http://cli.example.test:8080/x")).toBe("cli.example.test:8080");
    expect(feedHost("")).toBe("");
  });

  it("prompts once per ready version", () => {
    const base = { state: "ready", version: "1.2.0", autoPrompt: true, dismissedVersion: null };
    expect(shouldAutoPromptReady(base)).toBe(true);
    expect(shouldAutoPromptReady({ ...base, dismissedVersion: "1.2.0" })).toBe(false);
    expect(shouldAutoPromptReady({ ...base, dismissedVersion: "1.1.0" })).toBe(true);
    expect(shouldAutoPromptReady({ ...base, autoPrompt: false })).toBe(false);
    expect(shouldAutoPromptReady({ ...base, state: "downloading" })).toBe(false);
  });
});
