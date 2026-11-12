/**
 * Desktop feature mask from atlas-server.
 * Bit 0 (1) fleet, bit 1 (2) wiki, bit 2 (4) remote, bit 3 (8) agent CLI.
 * Fetched once at startup, again after a successful login or account switch.
 * The host returns 0 when the request fails. This module does the same.
 */

import { useEffect, useState } from "react";
import { invoke, isDesktopHost } from "./api/host";

export function featureBit(mask: number, bit: number): boolean {
  if (!Number.isInteger(mask) || mask < 0 || mask > 65535) return false;
  if (bit < 0 || bit > 15) return false;
  return (mask & (1 << bit)) !== 0;
}

type Snap = { mask: number; ready: boolean };

let snap: Snap = { mask: 0, ready: false };
const listeners = new Set<() => void>();
let started = false;
let inflight: Promise<void> | null = null;

function emit(): void {
  for (const fn of listeners) fn();
}

function setSnap(next: Snap): void {
  snap = next;
  emit();
}

export async function refreshFeatureMask(): Promise<void> {
  if (!isDesktopHost()) {
    setSnap({ mask: 0, ready: true });
    return;
  }
  if (inflight) return inflight;
  inflight = (async () => {
    try {
      const n = await invoke<number>("feature_mask_get");
      const mask =
        typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 65535
          ? n
          : 0;
      setSnap({ mask, ready: true });
    } catch {
      setSnap({ mask: 0, ready: true });
    } finally {
      inflight = null;
    }
  })();
  return inflight;
}

function ensureStarted(): void {
  if (started || typeof window === "undefined") return;
  started = true;
  void refreshFeatureMask();
}

export function useFeatureMask(): {
  ready: boolean;
  mask: number;
  fleet: boolean;
  wiki: boolean;
  remote: boolean;
  agentCli: boolean;
} {
  const [local, setLocal] = useState(snap);
  useEffect(() => {
    const pull = () => setLocal(snap);
    listeners.add(pull);
    ensureStarted();
    pull();
    return () => {
      listeners.delete(pull);
    };
  }, []);
  const mask = local.ready ? local.mask : 0;
  return {
    ready: local.ready,
    mask,
    fleet: featureBit(mask, 0),
    wiki: featureBit(mask, 1),
    remote: featureBit(mask, 2),
    agentCli: featureBit(mask, 3),
  };
}
