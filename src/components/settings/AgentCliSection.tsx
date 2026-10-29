/**
 * Settings → Agent CLIs. Profiles and the new-chat default.
 * Stored in agent-runtimes.json, separate from App settings.
 * Per-relay CLI is chosen on the Atlas Relay row.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Select, type SelectOption } from "@/components/Select";
import * as api from "@/lib/api";
import type { AgentRuntimesFile, RuntimeProfile } from "@/lib/api";
import { useSettingsModel } from "@/providers/SettingsModelContext";
import { UiSwitch } from "./shared";

function newProfile(kind: string): RuntimeProfile {
  return {
    id: crypto.randomUUID(),
    kind,
    label: kind,
    binary: "",
    home: "",
    model: "",
    permission: "yolo",
    enabled: true,
  };
}

export function AgentCliSection() {
  const { t, rowHighlight } = useSettingsModel();
  const [file, setFile] = useState<AgentRuntimesFile | null>(null);
  const [error, setError] = useState("");
  const fileRef = useRef<AgentRuntimesFile | null>(null);
  const saveGen = useRef(0);
  const saveTimer = useRef<number | null>(null);

  const remember = (next: AgentRuntimesFile) => {
    fileRef.current = next;
    setFile(next);
  };

  const reload = useCallback(async () => {
    if (!api.isDesktopHost()) return;
    try {
      const runtimes = await api.agentRuntimesGet();
      fileRef.current = runtimes;
      setFile(runtimes);
      setError("");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
    return () => {
      if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    };
  }, [reload]);

  const persist = (next: AgentRuntimesFile, immediate: boolean) => {
    remember(next);
    const gen = ++saveGen.current;
    if (saveTimer.current !== null) window.clearTimeout(saveTimer.current);
    const run = () => {
      saveTimer.current = null;
      void api
        .agentRuntimesSave(next)
        .then((saved) => {
          if (saveGen.current !== gen) return;
          remember(saved);
          setError("");
        })
        .catch((e) => {
          if (saveGen.current === gen) setError(String(e));
        });
    };
    if (immediate) run();
    else saveTimer.current = window.setTimeout(run, 400);
  };

  if (!file) {
    return null;
  }

  const kindOptions: SelectOption[] = [
    { value: "atlas", label: t("settings.agentCli.kindAtlas") },
    { value: "claude", label: t("settings.agentCli.kindClaude") },
    { value: "cursor", label: t("settings.agentCli.kindCursor") },
  ];
  const permissionOptions: SelectOption[] = [
    { value: "yolo", label: t("settings.agentCli.permissionYolo") },
    { value: "ask", label: t("settings.agentCli.permissionAsk") },
  ];
  const profileOptions: SelectOption[] = [
    { value: "", label: t("settings.agentCli.defaultEmpty") },
    ...file.profiles
      .filter((profile) => profile.enabled)
      .map((profile) => ({
        value: profile.id,
        label: profile.label || profile.kind,
      })),
  ];

  const patchProfile = (
    id: string,
    patch: Partial<RuntimeProfile>,
    immediate = false,
  ) => {
    const current = fileRef.current;
    if (!current) return;
    persist(
      {
        ...current,
        profiles: current.profiles.map((profile) =>
          profile.id === id ? { ...profile, ...patch } : profile,
        ),
      },
      immediate,
    );
  };

  return (
    <>
      <div
        className={
          "settings-card" + rowHighlight("settings-anchor-agentCliProfiles")
        }
        id="settings-anchor-agentCliProfiles"
      >
        <div className="settings-row settings-row--stack">
          <div className="settings-row__text">
            <div className="settings-row__label">
              {t("settings.agentCli.profiles")}
            </div>
            <div className="settings-row__desc">{t("settings.agentCli.intro")}</div>
          </div>
          {file.profiles.map((profile) => (
            <div key={profile.id} className="settings-row settings-row--stack">
              <input
                className="settings-input"
                value={profile.label}
                aria-label={t("settings.agentCli.label")}
                onChange={(e) => patchProfile(profile.id, { label: e.target.value })}
              />
              <Select
                value={profile.kind}
                options={kindOptions}
                aria-label={t("settings.agentCli.kind")}
                onChange={(kind) => patchProfile(profile.id, { kind }, true)}
              />
              <input
                className="settings-input"
                value={profile.binary}
                placeholder={t("settings.agentCli.binary")}
                aria-label={t("settings.agentCli.binary")}
                spellCheck={false}
                onChange={(e) => patchProfile(profile.id, { binary: e.target.value })}
              />
              <div className="settings-row">
                <button
                  type="button"
                  className="btn btn--ghost"
                  onClick={() => {
                    void api.pickCliBinary().then((path) => {
                      if (path) patchProfile(profile.id, { binary: path }, true);
                    });
                  }}
                >
                  {t("settings.agentCli.browse")}
                </button>
                <button
                  type="button"
                  className="btn btn--ghost"
                  onClick={() => {
                    void api.agentRuntimesProbe(profile.kind).then((hits) => {
                      if (hits[0]) patchProfile(profile.id, { binary: hits[0] }, true);
                      else setError(t("settings.agentCli.probeEmpty"));
                    });
                  }}
                >
                  {t("settings.agentCli.probe")}
                </button>
              </div>
              <input
                className="settings-input"
                value={profile.home}
                placeholder={t("settings.agentCli.home")}
                aria-label={t("settings.agentCli.home")}
                spellCheck={false}
                onChange={(e) => patchProfile(profile.id, { home: e.target.value })}
              />
              <input
                className="settings-input"
                value={profile.model}
                placeholder={t("settings.agentCli.model")}
                aria-label={t("settings.agentCli.model")}
                spellCheck={false}
                onChange={(e) => patchProfile(profile.id, { model: e.target.value })}
              />
              <Select
                value={profile.permission === "ask" ? "ask" : "yolo"}
                options={permissionOptions}
                aria-label={t("settings.agentCli.permission")}
                onChange={(permission) =>
                  patchProfile(profile.id, { permission }, true)
                }
              />
              <div className="settings-row">
                <span className="settings-row__label">
                  {t("settings.agentCli.enabled")}
                </span>
                <UiSwitch
                  checked={profile.enabled}
                  label={t("settings.agentCli.enabled")}
                  onChange={(enabled) => patchProfile(profile.id, { enabled }, true)}
                />
              </div>
              <button
                type="button"
                className="btn btn--ghost"
                onClick={() => {
                  const current = fileRef.current;
                  if (!current) return;
                  persist(
                    {
                      ...current,
                      profiles: current.profiles.filter((row) => row.id !== profile.id),
                      desktopDefaultProfileId:
                        current.desktopDefaultProfileId === profile.id
                          ? ""
                          : current.desktopDefaultProfileId,
                    },
                    true,
                  );
                }}
              >
                {t("settings.agentCli.remove")}
              </button>
            </div>
          ))}
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => {
              const current = fileRef.current;
              if (!current) return;
              persist(
                { ...current, profiles: [...current.profiles, newProfile("claude")] },
                true,
              );
            }}
          >
            {t("settings.agentCli.add")}
          </button>
          {error ? (
            <p className="settings-row__desc" role="alert">
              {error}
            </p>
          ) : null}
        </div>
      </div>

      <div
        className={"settings-card" + rowHighlight("settings-anchor-agentCliDefault")}
        id="settings-anchor-agentCliDefault"
      >
        <div className="settings-row settings-row--stack">
          <div className="settings-row__text">
            <div className="settings-row__label">{t("settings.agentCli.default")}</div>
          </div>
          <Select
            value={file.desktopDefaultProfileId}
            options={profileOptions}
            aria-label={t("settings.agentCli.default")}
            onChange={(desktopDefaultProfileId) => {
              const current = fileRef.current;
              if (!current) return;
              persist({ ...current, desktopDefaultProfileId }, true);
            }}
          />
        </div>
      </div>
    </>
  );
}
