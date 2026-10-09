import { describe, expect, it } from "vitest";
import { atlasCloudTaskIdFromHash, resolveWorkbenchHash } from "./workbenchHash";

describe("resolveWorkbenchHash", () => {
  it("treats empty / workbench / home as the chat pane", () => {
    expect(resolveWorkbenchHash("")).toEqual({ kind: "pane", pane: "chat" });
    expect(resolveWorkbenchHash("#/workbench")).toEqual({
      kind: "pane",
      pane: "chat",
    });
    expect(resolveWorkbenchHash("#/home")).toEqual({
      kind: "pane",
      pane: "chat",
    });
  });

  it("routes automations and kanban panes", () => {
    expect(resolveWorkbenchHash("#/automations")).toEqual({
      kind: "pane",
      pane: "automations",
    });
    expect(resolveWorkbenchHash("#/kanban/board")).toEqual({
      kind: "pane",
      pane: "kanban",
    });
    expect(resolveWorkbenchHash("#/octo")).toEqual({
      kind: "pane",
      pane: "octo",
    });
    expect(resolveWorkbenchHash("#/octo/inbox")).toEqual({
      kind: "pane",
      pane: "octo",
    });
    expect(resolveWorkbenchHash("#/atlas-cloud")).toEqual({
      kind: "pane",
      pane: "atlasCloud",
    });
    expect(resolveWorkbenchHash("#/atlas-cloud/tasks")).toEqual({
      kind: "pane",
      pane: "atlasCloud",
    });
    expect(resolveWorkbenchHash("#/atlas-cloud/task/prompt%201")).toEqual({
      kind: "pane",
      pane: "atlasCloud",
    });
    expect(atlasCloudTaskIdFromHash("#/atlas-cloud/task/prompt%201")).toBe(
      "prompt 1",
    );
    expect(atlasCloudTaskIdFromHash("#/atlas-cloud")).toBeNull();
    expect(atlasCloudTaskIdFromHash("#/atlas-cloud/tasks")).toBeNull();
  });

  it("bare settings hash restores last route", () => {
    expect(resolveWorkbenchHash("#/settings")).toEqual({
      kind: "settings-last",
    });
    expect(resolveWorkbenchHash("#/settings/not-a-section")).toEqual({
      kind: "settings-last",
    });
  });

  it("explicit settings section wins and parses PR hub query", () => {
    expect(resolveWorkbenchHash("#/settings/runtime/tools")).toEqual({
      kind: "settings-explicit",
      section: "runtime",
      tab: "tools",
      prNumber: null,
    });
    expect(resolveWorkbenchHash("#/settings/runtime/tools?pr=42")).toEqual({
      kind: "settings-explicit",
      section: "runtime",
      tab: "tools",
      prNumber: 42,
    });
  });
});
