import type { PetKind } from "./petFocus";
import type { PetTask } from "./petTasks";

/** Done-chip ids currently on the overlay. */
export function petDoneTaskIds(tasks: readonly PetTask[]): string[] {
  const ids: string[] = [];
  for (const row of tasks) {
    if (row.phase === "done" && row.sessionId) ids.push(row.sessionId);
  }
  return ids;
}

/**
 * Fire the colorful spin once when a session actually finishes.
 * Skip the first snapshot so opening the pet on an already-ready chat
 * does not replay the celebration.
 */
export function shouldTriggerPetSpin(input: {
  primed: boolean;
  prevKind: PetKind | null;
  nextKind: PetKind;
  prevDoneIds: ReadonlySet<string>;
  nextDoneIds: ReadonlySet<string>;
}): boolean {
  if (!input.primed) return false;
  if (input.nextKind === "ready" && input.prevKind !== "ready") return true;
  for (const id of input.nextDoneIds) {
    if (!input.prevDoneIds.has(id)) return true;
  }
  return false;
}
