/**
 * Poll the Atlas Relay agent bridge. Online is the only connected phase.
 */
import { useEffect, useState } from "react";
import { createT } from "@/i18n";
import {
  atlasRelayAgentStatus,
  atlasRelayTaskTrace,
  isTauri,
  type AtlasRelayAgentStatus,
  type AtlasRelayTaskTrace,
} from "@/lib/api";

type TFn = ReturnType<typeof createT>;

const POLL_MS = 2000;
const TRACE_POLL_MS = 500;

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

/**
 * Poll one relay task while its panel is open.
 * A running task keeps polling. A finished task is fetched once more, then stops.
 */
export function useAtlasRelayTaskTrace(
  agentId: string | null,
  taskId: string | null,
): AtlasRelayTaskTrace | null {
  const [trace, setTrace] = useState<AtlasRelayTaskTrace | null>(null);
  useEffect(() => {
    if (!isTauri() || !agentId || !taskId) {
      setTrace(null);
      return;
    }
    let stop = false;
    let timer = 0;
    const schedule = () => {
      timer = window.setTimeout(tick, TRACE_POLL_MS);
    };
    const tick = () => {
      void atlasRelayTaskTrace(agentId, taskId)
        .then((next) => {
          if (stop) return;
          setTrace(next);
          if (next.found && next.status !== "running") return;
          schedule();
        })
        .catch(() => {
          if (!stop) schedule();
        });
    };
    tick();
    return () => {
      stop = true;
      window.clearTimeout(timer);
    };
  }, [agentId, taskId]);
  return trace;
}

export function atlasCloudNavLabel(
  tr: TFn,
  phase: string | null | undefined,
): string {
  return phase === "online"
    ? tr("sidebar.atlasCloud.connected")
    : tr("sidebar.atlasCloud.disconnected");
}
