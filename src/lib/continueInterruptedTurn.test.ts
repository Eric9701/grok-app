import { describe, expect, it } from "vitest";
import {
  buildContinueAgentPrompt,
  isContinuableEndReason,
  latestContinuableEndMessageId,
} from "./continueInterruptedTurn";

describe("continueInterruptedTurn", () => {
  it("includes the pending command in a fence", () => {
    const text = buildContinueAgentPrompt({
      command: "git rev-parse master origin/master HEAD",
      title: "List commits to merge into hzh/dev",
      toolName: "run_terminal_command",
    });
    expect(text).toContain("git rev-parse master origin/master HEAD");
    expect(text).toContain("run_terminal_command");
    expect(text).toContain("```");
    expect(text).toMatch(/interrupted when the app host process restarted/i);
    expect(text).not.toMatch(/^继续上次中断的任务$/);
  });

  it("still asks to resume when the command is missing", () => {
    const text = buildContinueAgentPrompt({});
    expect(text).toMatch(/command is not available/i);
    expect(text).not.toContain("```");
  });

  it("marks host_exit and agent_exit as continuable", () => {
    expect(isContinuableEndReason("host_exit")).toBe(true);
    expect(isContinuableEndReason("agent_exit")).toBe(true);
    expect(isContinuableEndReason("user_stop")).toBe(false);
  });
});

describe("latestContinuableEndMessageId", () => {
  it("returns only the last host_exit after the last user", () => {
    const id = latestContinuableEndMessageId([
      { id: "u1", role: "user" },
      { id: "old", role: "tool", marker: "turn_cancelled", content: "turn_cancelled|host_exit", toolStatus: "host_exit" },
      { id: "u2", role: "user" },
      { id: "a2", role: "assistant" },
      { id: "new", role: "tool", marker: "turn_cancelled", content: "turn_cancelled|host_exit", toolStatus: "host_exit" },
    ]);
    expect(id).toBe("new");
  });

  it("ignores older chips once a later user turn exists", () => {
    const id = latestContinuableEndMessageId([
      { id: "u1", role: "user" },
      { id: "old", role: "tool", marker: "turn_cancelled", content: "turn_cancelled|agent_exit", toolStatus: "agent_exit" },
      { id: "u2", role: "user" },
      { id: "a2", role: "assistant" },
    ]);
    expect(id).toBeNull();
  });
});
