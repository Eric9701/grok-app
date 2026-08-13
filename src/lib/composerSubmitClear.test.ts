import { describe, expect, it } from "vitest";
import { shouldClearComposerAfterSubmit } from "./composerSubmitClear";

const file = { path: "/tmp/a.txt" };

describe("shouldClearComposerAfterSubmit", () => {
  it("clears when draft and attachments are unchanged", () => {
    expect(
      shouldClearComposerAfterSubmit({
        sentText: "hello",
        sentAttachments: [file],
        currentText: "hello",
        currentAttachments: [file],
      }),
    ).toBe(true);
  });

  it("keeps composer when the user typed during send", () => {
    expect(
      shouldClearComposerAfterSubmit({
        sentText: "hello",
        sentAttachments: [],
        currentText: "hello\nand more",
        currentAttachments: [],
      }),
    ).toBe(false);
  });

  it("keeps composer when attachments changed during send", () => {
    expect(
      shouldClearComposerAfterSubmit({
        sentText: "hello",
        sentAttachments: [file],
        currentText: "hello",
        currentAttachments: [file, { path: "/tmp/b.png" }],
      }),
    ).toBe(false);
    expect(
      shouldClearComposerAfterSubmit({
        sentText: "hello",
        sentAttachments: [file],
        currentText: "hello",
        currentAttachments: [],
      }),
    ).toBe(false);
  });

  it("clears an empty follow-up that still matches the sent empty attachments", () => {
    expect(
      shouldClearComposerAfterSubmit({
        sentText: "hello",
        sentAttachments: [],
        currentText: "hello",
        currentAttachments: [],
      }),
    ).toBe(true);
  });
});
