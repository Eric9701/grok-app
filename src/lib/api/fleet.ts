/** Fleet leader: queue, roster, and listen control. */

import { invoke } from "./host";

export type FleetMachine = {
  agentId: string;
  online: boolean;
  local: boolean;
};

export type FleetStatus = {
  enabled: boolean;
  savedBind: string;
  listening: boolean;
  listenError?: string | null;
  tokenLast4: string;
  machines: FleetMachine[];
};

export type FleetTask = {
  id: string;
  agentId: string;
  cwd: string;
  preview: string;
  status: string;
  error?: string | null;
  failureReason?: string | null;
  createdAtMs: number;
  finishedAtMs?: number | null;
  sessionId: string;
  retryOf?: string | null;
  unreachable: boolean;
};

export type FleetStep = {
  kind: string;
  text: string;
  toolCallId?: string | null;
  toolName?: string | null;
  title?: string | null;
  status?: string | null;
};

export type FleetTrace = {
  id: string;
  agentId: string;
  cwd: string;
  prompt: string;
  status: string;
  error?: string | null;
  failureReason?: string | null;
  sessionId: string;
  unreachable: boolean;
  steps: FleetStep[];
};

export type FleetListenApply = {
  listening: boolean;
  bind: string;
  error?: string | null;
  connectionCli?: string | null;
};

export function fleetStatus() {
  return invoke<FleetStatus>("fleet_status");
}

export function fleetTasks() {
  return invoke<FleetTask[]>("fleet_tasks");
}

export function fleetTaskTrace(id: string) {
  return invoke<FleetTrace>("fleet_task_trace", { id });
}

export function fleetDispatch(agentId: string, cwd: string, prompt: string) {
  return invoke<FleetTask>("fleet_dispatch", { agentId, cwd, prompt });
}

export function fleetCancel(id: string) {
  return invoke<void>("fleet_cancel", { id });
}

export function fleetListenApply(
  enabled: boolean,
  bind: string,
  token: string,
  sampleAgentId?: string,
) {
  return invoke<FleetListenApply>("fleet_listen_apply", {
    enabled,
    bind,
    token,
    sampleAgentId: sampleAgentId ?? null,
  });
}
