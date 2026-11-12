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
  /** When this compile was created. The kanban clock uses this. */
  startedAt: number;
  /** Last time the CLI process reported it was still running. */
  pulseAt: number | null;
  atMs: number;
};

const MAX_TASKS = 40;
const STORE_KEY = "__atlasWikiCompileTasks";
const BOUND_KEY = "__atlasWikiCompileEventsBound";

type Bucket = {
  tasks: WikiCompileTask[];
  listeners: Set<() => void>;
};

function bucket(): Bucket {
  const g = globalThis as typeof globalThis & { [STORE_KEY]?: Bucket };
  if (!g[STORE_KEY]) g[STORE_KEY] = { tasks: [], listeners: new Set() };
  return g[STORE_KEY];
}

function emit(next: WikiCompileTask[]) {
  const box = bucket();
  box.tasks = next;
  for (const listener of box.listeners) listener();
}

function replace(id: string, next: WikiCompileTask) {
  emit(bucket().tasks.map((task) => (task.id === id ? next : task)));
}

export function getWikiCompileTasks(): readonly WikiCompileTask[] {
  return bucket().tasks;
}

export function subscribeWikiCompileTasks(listener: () => void): () => void {
  const box = bucket();
  box.listeners.add(listener);
  return () => {
    box.listeners.delete(listener);
  };
}

export function formatWikiElapsed(ms: number): string {
  const sec = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(sec / 60);
  const seconds = sec % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

/** Quiet only after the process has reported once and then gone silent. */
export function wikiCompileActivity(task: WikiCompileTask, now: number): "live" | "quiet" {
  if (task.status !== "running" || task.pulseAt == null) return "live";
  return now - task.pulseAt > 8000 ? "quiet" : "live";
}

export function noteWikiCompilePulse(project: string, _elapsedMs: number) {
  const key = project.trim();
  if (!key) return;
  const now = Date.now();
  let changed = false;
  const next = bucket().tasks.map((task) => {
    if (task.status !== "running" || task.projectPath !== key) return task;
    changed = true;
    return { ...task, pulseAt: now, atMs: now };
  });
  if (changed) emit(next);
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
    startedAt: input.atMs ?? Date.now(),
    pulseAt: null,
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

const WIKI_CLI_PULSE_EVENT = "wiki://cli-pulse";

/** Listen once, on the shared store, so the kanban and the wiki page see the same pulses. */
export function ensureWikiCompileEvents() {
  const g = globalThis as typeof globalThis & { [BOUND_KEY]?: boolean };
  if (g[BOUND_KEY]) return;
  g[BOUND_KEY] = true;
  void import("@/lib/api").then((api) => {
    if (!api.isTauri()) return;
    void api.listen<{ project?: string; elapsedMs?: number }>(WIKI_CLI_PULSE_EVENT, (payload) => {
      if (!payload?.project) return;
      noteWikiCompilePulse(payload.project, payload.elapsedMs ?? 0);
    });
  });
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
  ensureWikiCompileEvents();
  emit(trim([task, ...bucket().tasks]));
  return ingestWithAtlas(
    input.projectPath,
    input.sourcePath,
    input.modelId,
    (stage) => {
      const current = getWikiCompileTasks().find((item) => item.id === id) ?? task;
      task = markWikiCompileStage(current, stage);
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
