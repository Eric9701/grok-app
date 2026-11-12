/**
 * Settings → Atlas: up to four Relay agents, each with its own address.
 *
 * Dials the relay and bridges ACP to a local Atlas CLI. Does not open a
 * workbench transcript.
 */
import { useCallback, useEffect, useState } from "react";
import { Select, type SelectOption } from "@/components/Select";
import * as api from "@/lib/api";
import type {
  AtlasRelayAgentLive,
  AtlasRelayAgentRecord,
  AtlasRelayAgentStatus,
  RuntimeProfile,
} from "@/lib/api";
import type { Vars } from "@/i18n";
import { parseAtlasRelayImport, splitRelayAddress } from "@/lib/atlasRelayImport";

type Props = {
  t: (k: string, vars?: Vars) => string;
  rowHighlight: (anchorId: string) => string;
};

type Row = {
  key: string;
  url: string;
  agentId: string;
  token: string;
  /** Catalog id. Empty follows the CLI default. */
  modelId: string;
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

function rowFromSaved(slot: AtlasRelayAgentRecord): Row {
  const id = slot.id?.trim() || "";
  return {
    key: id || `saved-${Math.random().toString(36).slice(2)}`,
    url: slot.url?.trim() || "",
    agentId: id,
    token: slot.token?.trim() || "",
    modelId: slot.modelId?.trim() || "",
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
    modelId: "",
    savedId: "",
    rowError: "",
  };
}

export function AtlasRelayAgentField({ t, rowHighlight }: Props) {
  const [rows, setRows] = useState<Row[]>([]);
  const [healthSecs, setHealthSecs] = useState("15");
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [fleet, setFleet] = useState<AtlasRelayAgentStatus | null>(null);
  const [catalog, setCatalog] = useState<{ id: string; label: string }[]>([]);
  const [profiles, setProfiles] = useState<RuntimeProfile[]>([]);
  const [profileByAgent, setProfileByAgent] = useState<Record<string, string>>({});
  const [draftProfile, setDraftProfile] = useState<Record<string, string>>({});
  const [importText, setImportText] = useState("");
  const [importInvalid, setImportInvalid] = useState(false);

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

  const refreshRuntimes = useCallback(async () => {
    if (!api.isDesktopHost()) return;
    try {
      const file = await api.agentRuntimesGet();
      setProfiles(file.profiles ?? []);
      const next: Record<string, string> = {};
      for (const binding of file.relayBindings ?? []) {
        const id = agentKey(binding.relayAgentId ?? "");
        if (id) next[id] = binding.profileId ?? "";
      }
      setProfileByAgent(next);
    } catch {
      /* keep the last list */
    }
  }, []);

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
    void refreshRuntimes();
    void refreshStatus();
    if (api.isTauri()) {
      void api
        .modelsListAvailable()
        .then((result) => {
          setCatalog(
            (result.models ?? [])
              .map((model) => ({
                id: model.id?.trim() || "",
                label: model.label?.trim() || model.id?.trim() || "",
              }))
              .filter((model) => model.id),
          );
        })
        .catch(() => {
          /* keep a saved id visible even when the catalog is empty */
        });
    }
    const id = window.setInterval(() => void refreshStatus(), 2000);
    return () => window.clearInterval(id);
  }, [refreshSettings, refreshRuntimes, refreshStatus]);

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

  const selectedProfile = (row: Row): string => {
    if (Object.prototype.hasOwnProperty.call(draftProfile, row.key)) {
      return draftProfile[row.key] ?? "";
    }
    const saved = agentKey(row.savedId);
    return saved ? profileByAgent[saved] ?? "" : "";
  };

  const showsAtlasModel = (profileId: string): boolean => {
    if (!profileId) return true;
    const profile = profiles.find((item) => item.id === profileId);
    return !profile || profile.kind === "atlas";
  };

  const runtimeOptions = (current: string): SelectOption[] => {
    const options: SelectOption[] = [
      { value: "", label: t("settings.agentCli.defaultEmpty") },
    ];
    const seen = new Set<string>([""]);
    for (const profile of profiles) {
      if (!profile.enabled || seen.has(profile.id)) continue;
      seen.add(profile.id);
      options.push({
        value: profile.id,
        label: profile.label.trim() || profile.kind,
      });
    }
    if (current && !seen.has(current)) {
      options.push({ value: current, label: current });
    }
    return options;
  };

  const changeRuntime = (row: Row, profileId: string) => {
    const saved = agentKey(row.savedId);
    if (!saved) {
      setDraftProfile((prev) => ({ ...prev, [row.key]: profileId }));
      return;
    }
    setDraftProfile((prev) => {
      if (!(row.key in prev)) return prev;
      const next = { ...prev };
      delete next[row.key];
      return next;
    });
    setProfileByAgent((prev) => ({ ...prev, [saved]: profileId }));
    if (!api.isTauri()) return;
    void api.agentRuntimesSetRelayBinding(saved, profileId).catch((e) => {
      patch(row.key, { rowError: String(e) });
    });
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
        await api.agentRuntimesSetRelayBinding(agentKey(row.savedId), "");
      }
      const chosen = selectedProfile(row);
      const already = agentKey(row.savedId) === id ? (profileByAgent[id] ?? "") : "";
      if (chosen && chosen !== already) {
        await api.agentRuntimesSetRelayBinding(id, chosen);
        setProfileByAgent((prev) => ({ ...prev, [id]: chosen }));
      }
      setFleet(
        await api.atlasRelayAgentConnect(
          row.url.trim(),
          id,
          row.token.trim(),
          row.modelId.trim(),
        ),
      );
      patch(row.key, { rowError: "", savedId: id, agentId: id });
      setDraftProfile((prev) => {
        if (!(row.key in prev)) return prev;
        const next = { ...prev };
        delete next[row.key];
        return next;
      });
      await refreshSettings();
      await refreshRuntimes();
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
        const saved = agentKey(row.savedId);
        const current = await api.settingsGet();
        await api.settingsSet({
          ...current,
          atlasRelayAgents: (current.atlasRelayAgents ?? []).filter(
            (slot) => agentKey(slot.id ?? "") !== saved,
          ),
        });
        await api.agentRuntimesSetRelayBinding(saved, "");
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

  const changeModel = (row: Row, modelId: string) => {
    patch(row.key, { modelId, rowError: "" });
    const savedId = agentKey(row.savedId);
    if (!savedId || !api.isTauri()) return;
    void api.atlasRelayAgentSetModel(savedId, modelId).catch((e) => {
      patch(row.key, { rowError: String(e) });
    });
  };

  const modelOptions = (row: Row): SelectOption[] => {
    const options: SelectOption[] = [
      { value: "", label: t("settings.atlasRelayAgentModelDefault") },
    ];
    const seen = new Set<string>([""]);
    for (const model of catalog) {
      if (seen.has(model.id)) continue;
      seen.add(model.id);
      options.push({ value: model.id, label: model.label || model.id });
    }
    if (row.modelId && !seen.has(row.modelId)) {
      options.push({ value: row.modelId, label: row.modelId });
    }
    return options;
  };

  const atCap = rows.length >= MAX_AGENTS;
  const blankDraft = [...rows]
    .reverse()
    .find((row) => !row.savedId && !row.url.trim() && !agentKey(row.agentId) && !row.token.trim());
  const canImport = !busyKey && (!atCap || !!blankDraft);

  const applyImport = () => {
    const parsed = parseAtlasRelayImport(importText);
    if (!parsed) {
      setImportInvalid(true);
      return;
    }
    const duplicate = rows.some(
      (row) => row.key !== blankDraft?.key && agentKey(row.agentId) === agentKey(parsed.agentId),
    );
    setImportInvalid(false);
    setImportText("");
    setRows((prev) => {
      const fill = (row: Row): Row => ({
        ...row,
        url: parsed.url,
        agentId: parsed.agentId,
        token: parsed.token,
        rowError: duplicate ? "duplicate" : "",
      });
      if (blankDraft && prev.some((row) => row.key === blankDraft.key)) {
        return prev.map((row) => (row.key === blankDraft.key ? fill(row) : row));
      }
      if (prev.length >= MAX_AGENTS) return prev;
      return [...prev, fill(blankRow())];
    });
  };

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
                  const split = splitRelayAddress(next);
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
              <Select
                value={selectedProfile(row)}
                options={runtimeOptions(selectedProfile(row))}
                onChange={(profileId) => changeRuntime(row, profileId)}
                aria-label={t("settings.agentCli.kind")}
                disabled={!!busyKey}
              />
              {showsAtlasModel(selectedProfile(row)) ? (
                <Select
                  value={row.modelId}
                  options={modelOptions(row)}
                  onChange={(modelId) => changeModel(row, modelId)}
                  aria-label={t("settings.atlasRelayAgentModel")}
                  disabled={!!busyKey}
                />
              ) : (
                <div className="settings-row__desc" role="status">
                  {t("settings.atlasRelayAgentModelDefault")}
                </div>
              )}
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
        <div className="settings-acp-field settings-acp-field--stack">
          <textarea
            className={"settings-input" + (importInvalid ? " is-invalid" : "")}
            rows={3}
            value={importText}
            placeholder={t("settings.atlasRelayAgentImportPh")}
            disabled={!canImport}
            spellCheck={false}
            autoComplete="off"
            aria-label={t("settings.atlasRelayAgentImport")}
            onChange={(e) => {
              setImportText(e.target.value);
              if (importInvalid) setImportInvalid(false);
            }}
          />
          <button
            type="button"
            className="btn btn--ghost"
            disabled={!canImport || !importText.trim()}
            onClick={applyImport}
          >
            {t("settings.atlasRelayAgentImportAction")}
          </button>
          {importInvalid ? (
            <div className="settings-row__desc" role="status">
              {t("settings.atlasRelayAgentImportInvalid")}
            </div>
          ) : null}
        </div>
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
