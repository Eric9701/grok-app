/**
 * Settings → Runtime → Connection: this app as an Atlas Relay agent.
 *
 * Dials the relay and bridges ACP to a local Atlas CLI. Does not open a
 * workbench transcript.
 */
import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { AtlasRelayAgentStatus } from "@/lib/api";
import type { Vars } from "@/i18n";

type Props = {
  t: (k: string, vars?: Vars) => string;
  rowHighlight: (anchorId: string) => string;
};

function phaseLabel(
  t: Props["t"],
  status: AtlasRelayAgentStatus | null,
): string {
  switch (status?.phase) {
    case "connecting":
      return t("settings.atlasRelayAgentConnecting");
    case "online":
      return t("settings.atlasRelayAgentOnline");
    case "reconnecting":
      return t("settings.atlasRelayAgentReconnecting");
    default:
      return t("settings.atlasRelayAgentStopped");
  }
}

/** Pull `token` / `agent_id` out of a pasted relay address. */
function splitRelayPaste(raw: string): {
  url: string;
  token?: string;
  agentId?: string;
} | null {
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

export function AtlasRelayAgentField({ t, rowHighlight }: Props) {
  const [url, setUrl] = useState("");
  const [agentId, setAgentId] = useState("");
  const [token, setToken] = useState("");
  const [healthSecs, setHealthSecs] = useState("15");
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<AtlasRelayAgentStatus | null>(null);

  const refreshSettings = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      const s = await api.settingsGet();
      setUrl(s.atlasRelayAgentUrl?.trim() || "");
      setAgentId(s.atlasRelayAgentId?.trim() || "");
      setToken(s.atlasRelayAgentToken?.trim() || "");
      const secs = s.atlasRelayAgentHealthSecs;
      setHealthSecs(String(secs && secs > 0 ? secs : 15));
    } catch {
      /* keep the draft */
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      setStatus(await api.atlasRelayAgentStatus());
    } catch {
      /* status is best-effort */
    }
  }, []);

  useEffect(() => {
    void refreshSettings();
    void refreshStatus();
    const id = window.setInterval(() => void refreshStatus(), 2000);
    return () => window.clearInterval(id);
  }, [refreshSettings, refreshStatus]);

  const live =
    status?.phase === "connecting" ||
    status?.phase === "online" ||
    status?.phase === "reconnecting";

  const connect = async () => {
    if (!api.isTauri() || busy) return;
    setBusy(true);
    try {
      setStatus(
        await api.atlasRelayAgentConnect(url.trim(), agentId.trim(), token.trim()),
      );
    } catch (e) {
      setStatus({
        phase: "stopped",
        error: String(e),
        cliAlive: false,
        agentId: agentId.trim(),
        url: url.trim(),
      });
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    if (!api.isTauri() || busy) return;
    setBusy(true);
    try {
      setStatus(await api.atlasRelayAgentDisconnect());
    } catch (e) {
      setStatus((prev) => ({
        phase: "stopped",
        error: String(e),
        cliAlive: false,
        agentId: prev?.agentId || agentId.trim(),
        url: prev?.url || url.trim(),
      }));
    } finally {
      setBusy(false);
    }
  };

  const commitHealth = async () => {
    if (!api.isTauri()) return;
    const n = Number(healthSecs);
    const secs = Number.isFinite(n) ? Math.trunc(n) : 0;
    try {
      const current = await api.settingsGet();
      const saved = await api.settingsSet({
        ...current,
        atlasRelayAgentHealthSecs: secs,
      });
      const next = saved.atlasRelayAgentHealthSecs;
      setHealthSecs(String(next && next > 0 ? next : 15));
    } catch {
      /* keep the draft */
    }
  };

  const ok = status?.phase === "online";
  const failed = !!status?.error && status.phase !== "online";

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-atlasRelayAgent")}
      id="settings-anchor-atlasRelayAgent"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("settings.atlasRelayAgent")}</div>
          <div className="settings-row__desc">{t("settings.atlasRelayAgentDesc")}</div>
        </div>
        <div className="settings-acp-field settings-acp-field--stack">
          <input
            className="settings-input"
            value={url}
            placeholder={t("settings.atlasRelayAgentUrlPh")}
            disabled={busy || live}
            onChange={(e) => {
              const next = e.target.value;
              const split = splitRelayPaste(next);
              if (!split) {
                setUrl(next);
                return;
              }
              setUrl(split.url);
              if (split.token) setToken(split.token);
              if (split.agentId && !agentId.trim()) setAgentId(split.agentId);
            }}
            aria-label={t("settings.atlasRelayAgent")}
          />
          <input
            className="settings-input"
            value={agentId}
            placeholder={t("settings.atlasRelayAgentIdPh")}
            disabled={busy || live}
            onChange={(e) => setAgentId(e.target.value)}
            aria-label={t("settings.atlasRelayAgentIdPh")}
          />
          <input
            className="settings-input"
            type="password"
            value={token}
            placeholder={t("settings.atlasRelayAgentTokenPh")}
            disabled={busy || live}
            onChange={(e) => setToken(e.target.value)}
            aria-label={t("settings.atlasRelayAgentTokenPh")}
            spellCheck={false}
            autoComplete="off"
          />
          {live ? (
            <button
              type="button"
              className="btn btn--ghost"
              disabled={busy}
              onClick={() => void disconnect()}
            >
              {t("settings.atlasRelayAgentDisconnect")}
            </button>
          ) : (
            <button
              type="button"
              className="btn btn--ghost"
              disabled={busy || !url.trim() || !agentId.trim()}
              onClick={() => void connect()}
            >
              {t("settings.atlasRelayAgentConnect")}
            </button>
          )}
          <label
            className="settings-row__text"
            id="settings-anchor-atlasRelayAgentHealth"
            htmlFor="atlas-relay-health-secs"
          >
            <span className="settings-row__label">
              {t("settings.atlasRelayAgentHealth")}
            </span>
            <span className="settings-row__desc">
              {t("settings.atlasRelayAgentHealthDesc")}
            </span>
          </label>
          <input
            id="atlas-relay-health-secs"
            className="settings-input"
            type="number"
            min={5}
            max={300}
            inputMode="numeric"
            value={healthSecs}
            aria-label={t("settings.atlasRelayAgentHealth")}
            onChange={(e) => setHealthSecs(e.target.value)}
            onBlur={() => void commitHealth()}
          />
        </div>
        {status ? (
          <div
            className={
              "settings-acp-chip" + (ok ? " is-ok" : failed ? " is-fail" : "")
            }
            role="status"
          >
            <span className="settings-acp-chip__dot" aria-hidden />
            <span className="settings-acp-chip__label">{phaseLabel(t, status)}</span>
            <span className="settings-acp-chip__meta">
              {status.cliAlive
                ? t("settings.atlasRelayAgentCli")
                : t("settings.atlasRelayAgentCliDown")}
              {status.error ? ` · ${status.error}` : ""}
            </span>
          </div>
        ) : null}
      </div>
    </div>
  );
}
