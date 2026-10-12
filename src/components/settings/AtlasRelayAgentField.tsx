/**
 * Settings → Atlas: up to four Relay agents, each with its own address.
 *
 * Dials the relay and bridges ACP to a local Atlas CLI. Does not open a
 * workbench transcript.
 */
import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { AtlasRelayAgentLive, AtlasRelayAgentRecord, AtlasRelayAgentStatus } from "@/lib/api";
import type { Vars } from "@/i18n";

type Props = {
  t: (k: string, vars?: Vars) => string;
  rowHighlight: (anchorId: string) => string;
};

type Row = {
  key: string;
  url: string;
  agentId: string;
  token: string;
  /** Id last saved. Empty until this row is stored. */
  savedId: string;
  /** `duplicate` or a host error string. */
  rowError: string;
};

const MAX_AGENTS = 4;

function phaseLabel(t: Props["t"], phase: string | undefined): string {
  switch (phase) {
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

function agentKey(id: string): string {
  return id.replace(/[\r\n]/g, "").trim().slice(0, 128);
}

function isLive(phase: string | undefined): boolean {
  return phase === "connecting" || phase === "online" || phase === "reconnecting";
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

function rowFromSaved(slot: AtlasRelayAgentRecord): Row {
  const id = slot.id?.trim() || "";
  return {
    key: id || `saved-${Math.random().toString(36).slice(2)}`,
    url: slot.url?.trim() || "",
    agentId: id,
    token: slot.token?.trim() || "",
    savedId: id,
    rowError: "",
  };
}

let draftSeq = 0;

function blankRow(): Row {
  draftSeq += 1;
  return {
    key: `draft-${draftSeq}`,
    url: "",
    agentId: "",
    token: "",
    savedId: "",
    rowError: "",
  };
}

export function AtlasRelayAgentField({ t, rowHighlight }: Props) {
  const [rows, setRows] = useState<Row[]>([]);
  const [healthSecs, setHealthSecs] = useState("15");
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [fleet, setFleet] = useState<AtlasRelayAgentStatus | null>(null);

  const applySaved = useCallback((agents: AtlasRelayAgentRecord[] | undefined) => {
    const saved = (agents ?? []).map(rowFromSaved).filter((r) => r.savedId);
    setRows((prev) => {
      const savedIds = new Set(saved.map((r) => agentKey(r.savedId)));
      const pending = prev.filter((r) => {
        if (r.savedId && savedIds.has(agentKey(r.savedId))) return false;
        return !savedIds.has(agentKey(r.agentId));
      });
      return [...saved, ...pending].slice(0, MAX_AGENTS);
    });
  }, []);

  const refreshSettings = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      const s = await api.settingsGet();
      applySaved(s.atlasRelayAgents);
      const secs = s.atlasRelayAgentHealthSecs;
      setHealthSecs(String(secs && secs > 0 ? secs : 15));
    } catch {
      /* keep the draft */
    }
  }, [applySaved]);

  const refreshStatus = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      setFleet(await api.atlasRelayAgentStatus());
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

  const liveOf = (agentId: string): AtlasRelayAgentLive | undefined =>
    fleet?.agents?.find((a) => a.agentId === agentKey(agentId));

  const patch = (key: string, next: Partial<Row>) => {
    setRows((prev) => prev.map((r) => (r.key === key ? { ...r, ...next } : r)));
  };

  const duplicateOf = (row: Row, agentId = row.agentId) => {
    const id = agentKey(agentId);
    if (!id) return false;
    return rows.some((other) => other.key !== row.key && agentKey(other.agentId) === id);
  };

  const connect = async (row: Row) => {
    if (!api.isTauri() || busyKey) return;
    const id = agentKey(row.agentId);
    if (!row.url.trim() || !id) return;
    if (duplicateOf(row)) {
      patch(row.key, { rowError: "duplicate" });
      return;
    }
    setBusyKey(row.key);
    try {
      if (row.savedId && agentKey(row.savedId) !== id) {
        const current = await api.settingsGet();
        await api.settingsSet({
          ...current,
          atlasRelayAgents: (current.atlasRelayAgents ?? []).filter(
            (slot) => agentKey(slot.id ?? "") !== agentKey(row.savedId),
          ),
        });
      }
      setFleet(
        await api.atlasRelayAgentConnect(row.url.trim(), id, row.token.trim()),
      );
      patch(row.key, { rowError: "", savedId: id, agentId: id });
      await refreshSettings();
    } catch (e) {
      patch(row.key, { rowError: String(e) });
    } finally {
      setBusyKey(null);
    }
  };

  const disconnect = async (row: Row) => {
    if (!api.isTauri() || busyKey) return;
    const id = agentKey(row.savedId || row.agentId);
    if (!id) return;
    setBusyKey(row.key);
    try {
      setFleet(await api.atlasRelayAgentDisconnect(id));
      patch(row.key, { rowError: "" });
    } catch (e) {
      patch(row.key, { rowError: String(e) });
    } finally {
      setBusyKey(null);
    }
  };

  const remove = async (row: Row) => {
    if (busyKey) return;
    const live = isLive(liveOf(row.agentId)?.phase);
    if (live) return;
    setBusyKey(row.key);
    try {
      if (row.savedId && api.isTauri()) {
        const current = await api.settingsGet();
        await api.settingsSet({
          ...current,
          atlasRelayAgents: (current.atlasRelayAgents ?? []).filter(
            (slot) => agentKey(slot.id ?? "") !== agentKey(row.savedId),
          ),
        });
      }
      setRows((prev) => prev.filter((r) => r.key !== row.key));
    } catch (e) {
      patch(row.key, { rowError: String(e) });
    } finally {
      setBusyKey(null);
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

  const atCap = rows.length >= MAX_AGENTS;

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
        {rows.map((row) => {
          const live = liveOf(row.agentId);
          const locked = !!busyKey || isLive(live?.phase);
          const duplicate = row.rowError === "duplicate" || duplicateOf(row);
          const hostError =
            row.rowError && row.rowError !== "duplicate" ? row.rowError : live?.error || "";
          const ok = live?.phase === "online";
          const failed = !!hostError && live?.phase !== "online";
          return (
            <div key={row.key} className="settings-acp-field settings-acp-field--stack">
              <input
                className="settings-input"
                value={row.url}
                placeholder={t("settings.atlasRelayAgentUrlPh")}
                disabled={locked}
                onChange={(e) => {
                  const next = e.target.value;
                  const split = splitRelayPaste(next);
                  if (!split) {
                    patch(row.key, { url: next, rowError: "" });
                    return;
                  }
                  const nextId = split.agentId ?? row.agentId;
                  const dup = !!split.agentId && duplicateOf(row, nextId);
                  patch(row.key, {
                    url: split.url,
                    token: split.token ?? row.token,
                    agentId: nextId,
                    rowError: dup ? "duplicate" : "",
                  });
                }}
                aria-label={t("settings.atlasRelayAgent")}
              />
              <input
                className="settings-input"
                value={row.agentId}
                placeholder={t("settings.atlasRelayAgentIdPh")}
                disabled={locked}
                onChange={(e) => {
                  const agentId = e.target.value;
                  patch(row.key, {
                    agentId,
                    rowError: duplicateOf(row, agentId) ? "duplicate" : "",
                  });
                }}
                aria-label={t("settings.atlasRelayAgentIdPh")}
              />
              <input
                className="settings-input"
                type="password"
                value={row.token}
                placeholder={t("settings.atlasRelayAgentTokenPh")}
                disabled={locked}
                onChange={(e) => patch(row.key, { token: e.target.value })}
                aria-label={t("settings.atlasRelayAgentTokenPh")}
                spellCheck={false}
                autoComplete="off"
              />
              {duplicate ? (
                <div className="settings-row__desc" role="status">
                  {t("settings.atlasRelayAgentDuplicate")}
                </div>
              ) : null}
              <div className="settings-acp-field">
                {isLive(live?.phase) ? (
                  <button
                    type="button"
                    className="btn btn--ghost"
                    disabled={!!busyKey}
                    onClick={() => void disconnect(row)}
                  >
                    {t("settings.atlasRelayAgentDisconnect")}
                  </button>
                ) : (
                  <button
                    type="button"
                    className="btn btn--ghost"
                    disabled={
                      !!busyKey || !row.url.trim() || !agentKey(row.agentId) || duplicate
                    }
                    onClick={() => void connect(row)}
                  >
                    {t("settings.atlasRelayAgentConnect")}
                  </button>
                )}
                <button
                  type="button"
                  className="btn btn--ghost"
                  disabled={!!busyKey || isLive(live?.phase)}
                  onClick={() => void remove(row)}
                >
                  {t("settings.atlasRelayAgentRemove")}
                </button>
              </div>
              <div
                className={
                  "settings-acp-chip" + (ok ? " is-ok" : failed ? " is-fail" : "")
                }
                role="status"
              >
                <span className="settings-acp-chip__dot" aria-hidden />
                <span className="settings-acp-chip__label">
                  {phaseLabel(t, live?.phase)}
                </span>
                <span className="settings-acp-chip__meta">
                  {live?.cliAlive
                    ? t("settings.atlasRelayAgentCli")
                    : t("settings.atlasRelayAgentCliDown")}
                  {hostError ? ` · ${hostError}` : ""}
                </span>
              </div>
            </div>
          );
        })}
        <button
          type="button"
          className="btn btn--ghost"
          disabled={atCap || !!busyKey}
          onClick={() => {
            if (atCap) return;
            setRows((prev) => [...prev, blankRow()].slice(0, MAX_AGENTS));
          }}
        >
          {t("settings.atlasRelayAgentAdd")}
        </button>
        {atCap ? (
          <div className="settings-row__desc">{t("settings.atlasRelayAgentMax")}</div>
        ) : null}
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
    </div>
  );
}
