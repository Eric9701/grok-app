/**
 * Poll the Atlas Relay agent bridge. Online is the only connected phase.
 */
import { useEffect, useState } from "react";
import { createT } from "@/i18n";
import { atlasRelayAgentStatus, isTauri, type AtlasRelayAgentStatus } from "@/lib/api";

type TFn = ReturnType<typeof createT>;

const POLL_MS = 2000;

export function useAtlasRelayStatus(): AtlasRelayAgentStatus | null {
  const [status, setStatus] = useState<AtlasRelayAgentStatus | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let stop = false;
    const tick = () => {
      void atlasRelayAgentStatus()
        .then((next) => {
          if (!stop) setStatus(next);
        })
        .catch(() => undefined);
    };
    tick();
    const id = window.setInterval(tick, POLL_MS);
    return () => {
      stop = true;
      window.clearInterval(id);
    };
  }, []);
  return status;
}

export function atlasCloudNavLabel(
  tr: TFn,
  phase: string | null | undefined,
): string {
  return phase === "online"
    ? tr("sidebar.atlasCloud.connected")
    : tr("sidebar.atlasCloud.disconnected");
}
