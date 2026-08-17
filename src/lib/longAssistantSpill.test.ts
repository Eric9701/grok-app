import { describe, expect, it, beforeEach } from "vitest";
import {
  LONG_ASSISTANT_PREVIEW_CHARS,
  LONG_ASSISTANT_SPILL_CHARS,
  clearSpillPathCache,
  getCachedSpillPath,
  previewLongAssistant,
  safeSpillFileStem,
  setCachedSpillPath,
  shouldSpillLongAssistant,
  spillCacheKey,
  utf8ToBase64,
} from "./longAssistantSpill";

describe("longAssistantSpill", () => {
  beforeEach(() => {
    clearSpillPathCache();
  });

  it("does not spill short replies", () => {
    expect(shouldSpillLongAssistant(0)).toBe(false);
    expect(shouldSpillLongAssistant(LONG_ASSISTANT_SPILL_CHARS - 1)).toBe(
      false,
    );
  });

  it("spills at the character threshold", () => {
    expect(shouldSpillLongAssistant(LONG_ASSISTANT_SPILL_CHARS)).toBe(true);
    expect(shouldSpillLongAssistant(LONG_ASSISTANT_SPILL_CHARS + 50_000)).toBe(
      true,
    );
  });

  it("preview is shorter than a spilling body and snaps to a newline", () => {
    const head = "alpha\nbeta\n";
    const rest = "x".repeat(LONG_ASSISTANT_SPILL_CHARS);
    const src = head + rest;
    const preview = previewLongAssistant(src, 20);
    expect(preview.length).toBeLessThan(src.length);
    expect(preview.endsWith("\n")).toBe(true);
    expect(preview.startsWith("alpha")).toBe(true);
    expect(preview.includes("beta")).toBe(true);
  });

  it("preview default width stays far below the spill threshold", () => {
    const src = "n".repeat(LONG_ASSISTANT_SPILL_CHARS);
    const preview = previewLongAssistant(src);
    expect(preview.length).toBeLessThanOrEqual(LONG_ASSISTANT_PREVIEW_CHARS + 1);
    expect(preview.length).toBeLessThan(LONG_ASSISTANT_SPILL_CHARS);
  });

  it("utf8ToBase64 round-trips CJK", () => {
    const src = "全文已保存为文本文件";
    const b64 = utf8ToBase64(src);
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    expect(new TextDecoder().decode(bytes)).toBe(src);
  });

  it("caches spill paths by message id + length", () => {
    const key = spillCacheKey("abc-123", 9000);
    expect(getCachedSpillPath(key)).toBeUndefined();
    setCachedSpillPath(key, "C:\\\\tmp\\\\assistant-abc.txt");
    expect(getCachedSpillPath(key)).toBe("C:\\\\tmp\\\\assistant-abc.txt");
  });

  it("sanitizes the file stem", () => {
    expect(safeSpillFileStem("msg/../evil id!")).toBe("assistant-msgevilid");
    expect(safeSpillFileStem("")).toBe("assistant-reply");
  });
});
