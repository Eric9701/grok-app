import { describe, expect, it } from "vitest";
import {
  buildContinueAgentPrompt,
  isContinuableEndReason,
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
