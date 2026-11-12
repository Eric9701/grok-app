import type { FleetMachine } from "@/lib/api/fleet";

/** Online remote machines replace the untouched `local` default. */
export function preferredAgent(
  machines: FleetMachine[],
  current: string,
  touched: boolean,
): string | null {
  if (touched || current !== "local") return null;
  const remote = machines.find((machine) => !machine.local && machine.online);
  return remote && remote.agentId !== current ? remote.agentId : null;
}
