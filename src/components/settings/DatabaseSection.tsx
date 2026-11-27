/** Settings → Database. Data directory, MCP injection, and the Tabularis service. */

import { useCallback, useEffect, useState } from "react";
import { Select } from "@/components/Select";
import * as api from "@/lib/api";
import { isTauri } from "@/lib/api/host";
import {
  tabularisRestart,
  tabularisStatus,
  tabularisStop,
  type TabularisStatus,
} from "@/lib/api/tabularis";
import { useSettingsModel } from "@/providers/SettingsModelContext";
import { UiSwitch } from "./shared";

export function DatabaseSection() {
  const { t, rowHighlight } = useSettingsModel();
  const [mode, setMode] = useState("shared");
  const [mcp, setMcp] = useState(true);
  const [binary, setBinary] = useState("");
  const [status, setStatus] = useState<TabularisStatus | null>(null);
  const [note, setNote] = useState("");

  const refresh = useCallback(() => {
    if (!isTauri()) return;
    void tabularisStatus().then(setStatus).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    void api.settingsGet().then((settings) => {
      setMode(settings.tabularisDataDirMode === "isolated" ? "isolated" : "shared");
      setMcp(settings.tabularisMcpEnabled !== false);
      setBinary(settings.tabularisBinaryPath ?? "");
    });
    refresh();
  }, [refresh]);

  const save = async (patch: {
    tabularisDataDirMode?: string;
    tabularisMcpEnabled?: boolean;
    tabularisBinaryPath?: string | null;
  }) => {
    const current = await api.settingsGet();
    await api.settingsSet({ ...current, ...patch });
    setNote(t("settings.database.saved"));
  };

  const statusLabel = () => {
    if (status?.state === "running") return t("settings.database.running");
    if (status?.error) return t("settings.database.missing");
    return t("settings.database.stopped");
  };

  return (
    <>
      <div
        className={"settings-card" + rowHighlight("settings-anchor-databaseDataDir")}
        id="settings-anchor-databaseDataDir"
      >
        <div className="settings-row settings-row--stack">
          <div className="settings-row__text">
            <div className="settings-row__label">{t("settings.database.dataDir")}</div>
            <div className="settings-row__desc">{t("settings.database.dataDirDesc")}</div>
          </div>
          <Select
            aria-label={t("settings.database.dataDir")}
            value={mode}
            options={[
              { value: "shared", label: t("settings.database.dataDirShared") },
              { value: "isolated", label: t("settings.database.dataDirIsolated") },
            ]}
            onChange={(value) => {
              setMode(value);
              void save({ tabularisDataDirMode: value });
            }}
          />
        </div>
      </div>
      <div
        className={"settings-card" + rowHighlight("settings-anchor-databaseMcp")}
        id="settings-anchor-databaseMcp"
      >
        <div className="settings-row">
          <div className="settings-row__text">
            <div className="settings-row__label">{t("settings.database.mcp")}</div>
            <div className="settings-row__desc">{t("settings.database.mcpDesc")}</div>
          </div>
          <UiSwitch
            checked={mcp}
            label={t("settings.database.mcp")}
            onChange={(next) => {
              setMcp(next);
              void save({ tabularisMcpEnabled: next });
            }}
          />
        </div>
      </div>
      <div
        className={"settings-card" + rowHighlight("settings-anchor-databaseBinary")}
        id="settings-anchor-databaseBinary"
      >
        <div className="settings-row settings-row--stack">
          <div className="settings-row__text">
            <div className="settings-row__label">{t("settings.database.binary")}</div>
            <div className="settings-row__desc">{t("settings.database.binaryDesc")}</div>
          </div>
          <input
            className="settings-input"
            value={binary}
            placeholder={t("settings.database.binaryPlaceholder")}
            aria-label={t("settings.database.binary")}
            onChange={(event) => setBinary(event.target.value)}
            onBlur={() => {
              void save({ tabularisBinaryPath: binary.trim() || null });
            }}
          />
        </div>
      </div>
      <div
        className={"settings-card" + rowHighlight("settings-anchor-databaseStatus")}
        id="settings-anchor-databaseStatus"
      >
        <div className="settings-row">
          <div className="settings-row__text">
            <div className="settings-row__label">{t("settings.database.status")}</div>
            <div className="settings-row__desc">
              {statusLabel()}
              {status?.error ? ` — ${status.error}` : ""}
              {note ? ` — ${note}` : ""}
            </div>
          </div>
          <button
            type="button"
            className="btn"
            onClick={() => {
              void tabularisRestart().then(setStatus).catch(() => refresh());
            }}
          >
            {t("settings.database.restart")}
          </button>
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => {
              void tabularisStop().then(setStatus).catch(() => refresh());
            }}
          >
            {t("settings.database.stop")}
          </button>
        </div>
      </div>
    </>
  );
}
