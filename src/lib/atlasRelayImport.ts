/**
 * Split a relay address or an `atlas agent … --grok-ws-url` command
 * into the address, agent id, and token stored on one settings row.
 */

export type RelayAddressParts = {
  url: string;
  token?: string;
  agentId?: string;
};

export type AtlasRelayImport = {
  url: string;
  agentId: string;
  token: string;
};

/** Pull `token` / `agent_id` out of a pasted relay address. */
export function splitRelayAddress(raw: string): RelayAddressParts | null {
  if (!raw.includes("token=") && !raw.includes("agent_id=")) return null;
  try {
    const trimmed = raw.trim();
    const withScheme = trimmed.includes("://") ? trimmed : `ws://${trimmed}`;
    const parsed = new URL(withScheme);
    const token = parsed.searchParams.get("token")?.trim() || "";
    const agentId = parsed.searchParams.get("agent_id")?.trim() || "";
    if (!token && !agentId) return null;
    parsed.searchParams.delete("token");
    parsed.searchParams.delete("agent_id");
    return {
      url: parsed.toString(),
      token: token || undefined,
      agentId: agentId || undefined,
    };
  } catch {
    return null;
  }
}

function flagValue(raw: string, flag: string): string | null {
  const re = new RegExp(
    `${flag}(?:\\s+|=)\\s*(?:"([^"]*)"|'([^']*)'|(\\S+))`,
    "i",
  );
  const match = re.exec(raw);
  const value = (match?.[1] ?? match?.[2] ?? match?.[3] ?? "").trim();
  return value || null;
}

function embeddedRelayUrl(raw: string): string | null {
  const match = raw.match(/[a-z][a-z0-9+.-]*:\/\/\S+/i);
  if (!match) return null;
  return match[0].replace(/['"]+$/g, "");
}

/**
 * Read url, agent id, and token from a CLI launch line or a bare socket URL.
 * `--grok-ws-origin` is not the socket. An empty token is allowed.
 */
export function parseAtlasRelayImport(raw: string): AtlasRelayImport | null {
  const text = raw.replace(/\r\n/g, "\n").trim();
  if (!text) return null;
  const candidate =
    flagValue(text, "--grok-ws-url") ?? embeddedRelayUrl(text) ?? text;
  const split = splitRelayAddress(candidate);
  const agentId = split?.agentId?.replace(/[\r\n]/g, "").trim().slice(0, 128) || "";
  const url = split?.url.trim() || "";
  if (!agentId || !url) return null;
  return {
    url,
    agentId,
    token: split?.token?.trim() || "",
  };
}
