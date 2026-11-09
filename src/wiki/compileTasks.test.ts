import { describe, expect, it } from "vitest";
import {
  createWikiCompileTask,
  failWikiCompileTask,
  finishWikiCompileTask,
  markWikiCompileStage,
} from "./compileTasks";

function task() {
  return createWikiCompileTask({
    id: "1",
    sourcePath: "raw/sources/note.md",
    projectName: "笔记",
    projectPath: "E:/knowledge1",
  });
}

describe("wiki compile tasks", () => {
  it("advances the active stage and fails that stage", () => {
    const running = markWikiCompileStage(task(), "analyze");
    expect(running.steps.map((step) => step.status)).toEqual([
      "completed",
      "in_progress",
      "pending",
      "pending",
    ]);
    const failed = failWikiCompileTask(running, "wiki_missing");
    expect(failed.status).toBe("failed");
    expect(failed.error).toBe("wiki_missing");
    expect(failed.steps[1]?.status).toBe("failed");
  });

  it("cancels the model stages when the source is unchanged", () => {
    const done = finishWikiCompileTask(markWikiCompileStage(task(), "read"), {
      skipped: true,
      written: [],
      usage: null,
      modelId: "",
    });
    expect(done.status).toBe("done");
    expect(done.skipped).toBe(true);
    expect(done.steps.map((step) => step.status)).toEqual([
      "completed",
      "cancelled",
      "cancelled",
      "cancelled",
    ]);
  });
});
