/**
 * Pet focus subscriptions. liveMap ticks do not fire when an idle snapshot
 * is unchanged — unread-clear must have its own listener or the overlay
 * stays on "ready/notifying" after the user views the session.
 */

import { sessionLiveMapStore } from "@/lib/sessionLiveMapStore";
import { getFinishedTurns, subscribeFinishedTurns } from "@/lib/sessionFinishedTurns";
import {
  loadUnreadSessionIds,
  SESSION_UNREAD_CHANGE_EVENT,
} from "@/lib/sessionUnread";
import { resolvePetFocus, type PetFocus, type PetFocusSession } from "./petFocus";
import { collectPetTasks, samePetTasks, type PetTask } from "./petTasks";

export type PetFocusBridgeOpts = {
  isEnabled: () => boolean;
  getSessions: () => readonly PetFocusSession[];
  push: (focus: PetFocus) => void;
  pushTasks?: (tasks: PetTask[]) => void;
};

export type PetFocusBridge = {
  tick: () => void;
  stop: () => void;
};

export function startPetFocusBridge(opts: PetFocusBridgeOpts): PetFocusBridge {
  let prev: PetFocus | null = null;
  let prevTasks: PetTask[] = [];
  let stopped = false;

  const tick = () => {
    if (stopped || !opts.isEnabled()) return;
    const input = {
      liveMap: sessionLiveMapStore.getMap(),
      unreadIds: loadUnreadSessionIds(),
      finishedTurns: getFinishedTurns(),
      sessions: opts.getSessions(),
    };
    const next = resolvePetFocus(prev, input);
    const tasks = collectPetTasks(input);
    if (opts.pushTasks && !samePetTasks(prevTasks, tasks)) {
      prevTasks = tasks;
      opts.pushTasks(tasks);
    }
    if (
      prev &&
      prev.kind === next.kind &&
      prev.sessionId === next.sessionId &&
      prev.toolTitle === next.toolTitle
    ) {
      prev = next;
      return;
    }
    prev = next;
    opts.push(next);
  };

  const unsubMap = sessionLiveMapStore.subscribeMap(tick);
  const unsubFin = subscribeFinishedTurns(tick);
  const onUnread = () => tick();
  if (typeof window !== "undefined" && typeof window.addEventListener === "function") {
    window.addEventListener(SESSION_UNREAD_CHANGE_EVENT, onUnread);
  }
  tick();

  return {
    tick,
    stop() {
      if (stopped) return;
      stopped = true;
      unsubMap();
      unsubFin();
      if (
        typeof window !== "undefined" &&
        typeof window.removeEventListener === "function"
      ) {
        window.removeEventListener(SESSION_UNREAD_CHANGE_EVENT, onUnread);
      }
    },
  };
}
