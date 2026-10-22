/** API domain: multi-CLI agent runtimes. */

import { invoke, isDesktopHost } from "./host";

export interface RuntimeProfile {
  id: string;
  kind: string;
  label: string;
  binary: string;
  home: string;
  model: string;
  permission: string;
  enabled: boolean;
}

export interface RuntimeSessionBinding {
  sessionId: string;
  profileId: string;
  cliSessionId?: string | null;
}

export interface RuntimeRelayBinding {
  relayAgentId: string;
  profileId: string;
}

export interface AgentRuntimesFile {
  profiles: RuntimeProfile[];
  desktopDefaultProfileId: string;
  sessionBindings: RuntimeSessionBinding[];
  relayBindings: RuntimeRelayBinding[];
}

export const AGENT_RUNTIMES_EVENT = "agent-runtimes-changed";

export function emptyAgentRuntimes(): AgentRuntimesFile {
  return {
    profiles: [],
    desktopDefaultProfileId: "",
    sessionBindings: [],
    relayBindings: [],
  };
}

export function publishAgentRuntimesChanged(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(AGENT_RUNTIMES_EVENT));
}

export async function agentRuntimesGet(): Promise<AgentRuntimesFile> {
  if (!isDesktopHost()) return emptyAgentRuntimes();
  const file = await invoke<AgentRuntimesFile>("agent_runtimes_get");
  return {
    profiles: file.profiles ?? [],
    desktopDefaultProfileId: file.desktopDefaultProfileId ?? "",
    sessionBindings: file.sessionBindings ?? [],
    relayBindings: file.relayBindings ?? [],
  };
}

export async function agentRuntimesSave(
  file: AgentRuntimesFile,
): Promise<AgentRuntimesFile> {
  const saved = await invoke<AgentRuntimesFile>("agent_runtimes_save", { file });
  publishAgentRuntimesChanged();
  return saved;
}

export async function agentRuntimesProbe(kind: string): Promise<string[]> {
  return invoke<string[]>("agent_runtimes_probe", { kind });
}

export async function agentRuntimesSetSessionBinding(
  sessionId: string,
  profileId: string,
): Promise<AgentRuntimesFile> {
  const saved = await invoke<AgentRuntimesFile>(
    "agent_runtimes_set_session_binding",
    { sessionId, profileId },
  );
  publishAgentRuntimesChanged();
  return saved;
}

export async function agentRuntimesSetRelayBinding(
  agentId: string,
  profileId: string,
): Promise<AgentRuntimesFile> {
  const saved = await invoke<AgentRuntimesFile>(
    "agent_runtimes_set_relay_binding",
    { agentId, profileId },
  );
  publishAgentRuntimesChanged();
  return saved;
}
