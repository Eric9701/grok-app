import { describe, expect, it } from "vitest";
import {
  nextComposerSubmitSettlement,
  shouldClearComposerAfterSubmit,
} from "./composerSubmitClear";

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

describe("nextComposerSubmitSettlement", () => {
  it("persists-clears after success when the composer was already optimistic-cleared", () => {
    expect(
      nextComposerSubmitSettlement({
        sendSucceeded: true,
        sentText: "hello",
        sentAttachments: [file],
        currentText: "",
        currentAttachments: [],
      }),
    ).toBe("persist-clear");
  });

  it("restores the sent payload after failure when the composer is still empty", () => {
    expect(
      nextComposerSubmitSettlement({
        sendSucceeded: false,
        sentText: "hello",
        sentAttachments: [file],
        currentText: "",
        currentAttachments: [],
      }),
    ).toBe("restore");
  });

  it("restores when React has not flushed the optimistic clear yet", () => {
    expect(
      nextComposerSubmitSettlement({
        sendSucceeded: false,
        sentText: "hello",
        sentAttachments: [file],
        currentText: "hello",
        currentAttachments: [file],
      }),
    ).toBe("restore");
  });

  it("leaves follow-up text typed during a slow send", () => {
    expect(
      nextComposerSubmitSettlement({
        sendSucceeded: true,
        sentText: "hello",
        sentAttachments: [],
        currentText: "next turn",
        currentAttachments: [],
      }),
    ).toBe("leave");
    expect(
      nextComposerSubmitSettlement({
        sendSucceeded: false,
        sentText: "hello",
        sentAttachments: [],
        currentText: "next turn",
        currentAttachments: [],
      }),
    ).toBe("leave");
  });

  it("leaves follow-up attachments added during a slow send", () => {
    expect(
      nextComposerSubmitSettlement({
        sendSucceeded: true,
        sentText: "hello",
        sentAttachments: [],
        currentText: "",
        currentAttachments: [file],
      }),
    ).toBe("leave");
  });
});
