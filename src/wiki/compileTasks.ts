/** In-memory knowledge-base compiles for the agent kanban. */

import type { IngestResult, IngestStage } from "./engine";
import { ingestWithAtlas } from "./host";

export const WIKI_COMPILE_STAGES = ["read", "analyze", "generate", "write"] as const;

export type WikiCompileStage = (typeof WIKI_COMPILE_STAGES)[number];

export type WikiCompileStepStatus = "pending" | "in_progress" | "completed" | "failed" | "cancelled";

export type WikiCompileStep = {
  id: WikiCompileStage;
  status: WikiCompileStepStatus;
};

export type WikiCompileTask = {
  id: string;
  sourcePath: string;
  projectName: string;
  projectPath: string;
  modelId: string;
  status: "running" | "done" | "failed";
  error: string;
  steps: WikiCompileStep[];
  written: string[];
  skipped: boolean;
  totalTokens: number | null;
  atMs: number;
};

const MAX_TASKS = 40;

let tasks: WikiCompileTask[] = [];
const listeners = new Set<() => void>();

function emit(next: WikiCompileTask[]) {
  tasks = next;
  for (const listener of listeners) listener();
}

function replace(id: string, next: WikiCompileTask) {
  emit(tasks.map((task) => (task.id === id ? next : task)));
}

export function getWikiCompileTasks(): readonly WikiCompileTask[] {
  return tasks;
}

export function subscribeWikiCompileTasks(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetWikiCompileTasksForTests() {
  emit([]);
}

export function createWikiCompileTask(input: {
  id: string;
  sourcePath: string;
  projectName: string;
  projectPath: string;
  modelId?: string;
  atMs?: number;
}): WikiCompileTask {
  return {
    id: input.id,
    sourcePath: input.sourcePath,
    projectName: input.projectName,
    projectPath: input.projectPath,
    modelId: input.modelId?.trim() ?? "",
    status: "running",
    error: "",
    steps: WIKI_COMPILE_STAGES.map((id) => ({ id, status: "pending" as const })),
    written: [],
    skipped: false,
    totalTokens: null,
    atMs: input.atMs ?? Date.now(),
  };
}

export function markWikiCompileStage(task: WikiCompileTask, stage: IngestStage): WikiCompileTask {
  const index = WIKI_COMPILE_STAGES.indexOf(stage);
  return {
    ...task,
    status: "running",
    atMs: Date.now(),
    steps: task.steps.map((step, i) => ({
      ...step,
      status: i < index ? "completed" : i === index ? "in_progress" : "pending",
    })),
  };
}

export function finishWikiCompileTask(task: WikiCompileTask, result: IngestResult): WikiCompileTask {
  return {
    ...task,
    status: "done",
    error: "",
    skipped: result.skipped,
    written: result.written,
    totalTokens: result.usage?.totalTokens ?? null,
    modelId: result.modelId || task.modelId,
    atMs: Date.now(),
    steps: task.steps.map((step, index) => ({
      ...step,
      status: result.skipped ? (index === 0 ? "completed" : "cancelled") : "completed",
    })),
  };
}

export function failWikiCompileTask(task: WikiCompileTask, err: unknown): WikiCompileTask {
  const error =
    typeof err === "string" ? err.trim() : err instanceof Error ? err.message.trim() : "wiki_failed";
  let marked = false;
  return {
    ...task,
    status: "failed",
    error: error || "wiki_failed",
    atMs: Date.now(),
    steps: task.steps.map((step) => {
      if (marked) return step;
      if (step.status === "in_progress" || step.status === "pending") {
        marked = true;
        return { ...step, status: "failed" };
      }
      return step;
    }),
  };
}

function trim(list: WikiCompileTask[]): WikiCompileTask[] {
  if (list.length <= MAX_TASKS) return list;
  const running = list.filter((task) => task.status === "running");
  const rest = list.filter((task) => task.status !== "running");
  return [...running, ...rest].slice(0, MAX_TASKS);
}

export function startWikiCompile(input: {
  projectPath: string;
  projectName: string;
  sourcePath: string;
  modelId?: string;
  force?: boolean;
}): Promise<IngestResult> {
  const id = `w${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
  let task = markWikiCompileStage(
    createWikiCompileTask({
      id,
      sourcePath: input.sourcePath,
      projectName: input.projectName,
      projectPath: input.projectPath,
      modelId: input.modelId,
    }),
    "read",
  );
  emit(trim([task, ...tasks]));
  return ingestWithAtlas(
    input.projectPath,
    input.sourcePath,
    input.modelId,
    (stage) => {
      task = markWikiCompileStage(task, stage);
      replace(id, task);
    },
    input.force,
  )
    .then((result) => {
      replace(id, finishWikiCompileTask(task, result));
      return result;
    })
    .catch((err: unknown) => {
      replace(id, failWikiCompileTask(task, err));
      throw err;
    });
}
