import { describe, expect, it, vi } from "vitest";

vi.mock("./host", () => ({
  ingestWithAtlas: () => new Promise(() => {}),
}));
import {
  createWikiCompileTask,
  failWikiCompileTask,
  finishWikiCompileTask,
  formatWikiElapsed,
  getWikiCompileTasks,
  markWikiCompileStage,
  noteWikiCompilePulse,
  resetWikiCompileTasksForTests,
  startWikiCompile,
  wikiCompileActivity,
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

  it("keeps a running compile on the shared list and marks a process pulse", () => {
    resetWikiCompileTasksForTests();
    void startWikiCompile({
      projectPath: "E:/knowledge1",
      projectName: "笔记",
      sourcePath: "raw/sources/note.md",
    });
    const running = getWikiCompileTasks()[0];
    expect(running?.status).toBe("running");
    expect(running?.sourcePath).toBe("raw/sources/note.md");
    noteWikiCompilePulse("E:/knowledge1", 2000);
    const pulsed = getWikiCompileTasks()[0]!;
    expect(pulsed.pulseAt).toBeTypeOf("number");
    expect(wikiCompileActivity(pulsed, Date.now())).toBe("live");
    expect(wikiCompileActivity(pulsed, Date.now() + 9000)).toBe("quiet");
    expect(formatWikiElapsed(12_500)).toBe("0:12");
    resetWikiCompileTasksForTests();
  });
});
