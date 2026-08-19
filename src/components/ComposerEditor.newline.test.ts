import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  storedTextToEditorNodes,
} from "./ComposerEditor";

const composerEditorSrc = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "ComposerEditor.tsx"),
  "utf8",
);

describe("storedTextToEditorNodes", () => {
  it("keeps a single line as one text node", () => {
    expect(storedTextToEditorNodes("hello")).toEqual([
      { type: "text", value: "hello" },
    ]);
  });

  it("adds a caret pad after the first trailing newline", () => {
    expect(storedTextToEditorNodes("hello\n")).toEqual([
      { type: "text", value: "hello" },
      { type: "br" },
      { type: "text", value: "\u200B" },
    ]);
  });

  it("keeps a blank line as two breaks plus a pad", () => {
    expect(storedTextToEditorNodes("hello\n\n")).toEqual([
      { type: "text", value: "hello" },
      { type: "br" },
      { type: "br" },
      { type: "text", value: "\u200B" },
    ]);
  });

  it("lets a leading newline land the caret on the second line", () => {
    expect(storedTextToEditorNodes("\n")).toEqual([
      { type: "br" },
      { type: "text", value: "\u200B" },
    ]);
  });

  it("does not pad a string that does not end in a newline", () => {
    expect(storedTextToEditorNodes("a\nb")).toEqual([
      { type: "text", value: "a" },
      { type: "br" },
      { type: "text", value: "b" },
    ]);
  });
});

describe("Shift+Enter must not rewrite the editor from a stale snapshot", () => {
  it("does not take lastValue as the Enter document then re-project", () => {
    // Production bug: preventDefault + lastValue + renderSegmentsInto wiped
    // live typed text that had not been committed to React yet.
    expect(composerEditorSrc).not.toMatch(
      /const draft = lastValue\.current;\s*\n\s*const caret = getComposerCaretIndex/,
    );
    expect(composerEditorSrc).toMatch(/insertComposerLineBreakInPlace/);
  });
});
