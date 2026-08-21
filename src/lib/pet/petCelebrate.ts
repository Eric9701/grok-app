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
 * Fire the colorful spin once when live work ends in an unread-ready pet.
 * Skip the first snapshot so opening the pet on an already-ready chat
 * does not replay the celebration. Mid-turn chips and peer completions
 * while another session is still live must not retrigger ribbons.
 */
export function shouldTriggerPetSpin(input: {
  primed: boolean;
  prevKind: PetKind | null;
  nextKind: PetKind;
}): boolean {
  if (!input.primed) return false;
  if (input.nextKind !== "ready") return false;
  return input.prevKind !== "ready";
}
