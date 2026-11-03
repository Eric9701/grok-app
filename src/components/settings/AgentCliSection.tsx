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
  const [claudeModels, setClaudeModels] = useState<Record<string, string>>({});
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

  useEffect(() => {
    if (!file || !api.isDesktopHost()) return;
    const targets = file.profiles.filter(
      (profile) => profile.kind === "claude" && !profile.model.trim(),
    );
    if (targets.length === 0) {
      setClaudeModels({});
      return;
    }
    let stop = false;
    void Promise.all(
      targets.map(async (profile) => {
        const model = await api.agentRuntimesClaudeModel(profile.home);
        return [profile.id, model ?? ""] as const;
      }),
    ).then((rows) => {
      if (stop) return;
      const next: Record<string, string> = {};
      for (const [id, model] of rows) {
        if (model) next[id] = model;
      }
      setClaudeModels(next);
    }).catch(() => {
      if (!stop) setClaudeModels({});
    });
    return () => {
      stop = true;
    };
  }, [file]);

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
        <div className="settings-row settings-row--top">
          <div className="settings-row__text">
            <div className="settings-row__label">
              {t("settings.agentCli.profiles")}
            </div>
            <div className="settings-row__desc">{t("settings.agentCli.intro")}</div>
            {error ? (
              <div className="settings-row__desc" role="alert">
                {error}
              </div>
            ) : null}
          </div>
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
        </div>
        {file.profiles.map((profile) => (
          <div key={profile.id} className="agent-cli-profile">
            <div className="agent-cli-profile__head">
              <input
                className="settings-input"
                value={profile.label}
                aria-label={t("settings.agentCli.label")}
                onChange={(e) => patchProfile(profile.id, { label: e.target.value })}
              />
              <Select
                className="agent-cli-profile__kind"
                value={profile.kind}
                options={kindOptions}
                aria-label={t("settings.agentCli.kind")}
                onChange={(kind) => patchProfile(profile.id, { kind }, true)}
              />
              <span className="agent-cli-profile__enable">
                <span>{t("settings.agentCli.enabled")}</span>
                <UiSwitch
                  checked={profile.enabled}
                  label={t("settings.agentCli.enabled")}
                  onChange={(enabled) => patchProfile(profile.id, { enabled }, true)}
                />
              </span>
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
            <div className="agent-cli-profile__grid">
              <div className="agent-cli-profile__field agent-cli-profile__field--wide">
                <span className="agent-cli-profile__label">
                  {t("settings.agentCli.binary")}
                </span>
                <div className="agent-cli-profile__line">
                  <input
                    className="settings-input"
                    value={profile.binary}
                    aria-label={t("settings.agentCli.binary")}
                    spellCheck={false}
                    onChange={(e) =>
                      patchProfile(profile.id, { binary: e.target.value })
                    }
                  />
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
              </div>
              <div className="agent-cli-profile__field">
                <span className="agent-cli-profile__label">
                  {t("settings.agentCli.home")}
                </span>
                <input
                  className="settings-input"
                  value={profile.home}
                  aria-label={t("settings.agentCli.home")}
                  spellCheck={false}
                  onChange={(e) => patchProfile(profile.id, { home: e.target.value })}
                />
              </div>
              <div className="agent-cli-profile__field">
                <span className="agent-cli-profile__label">
                  {t("settings.agentCli.model")}
                </span>
                <input
                  className="settings-input"
                  value={profile.model}
                  aria-label={t("settings.agentCli.model")}
                  spellCheck={false}
                  onChange={(e) => patchProfile(profile.id, { model: e.target.value })}
                />
                {profile.kind === "claude" &&
                !profile.model.trim() &&
                claudeModels[profile.id] ? (
                  <span className="agent-cli-profile__hint">
                    {claudeModels[profile.id]}
                  </span>
                ) : null}
              </div>
              <div className="agent-cli-profile__field">
                <span className="agent-cli-profile__label">
                  {t("settings.agentCli.permission")}
                </span>
                <Select
                  className="agent-cli-profile__select"
                  value={profile.permission === "ask" ? "ask" : "yolo"}
                  options={permissionOptions}
                  aria-label={t("settings.agentCli.permission")}
                  onChange={(permission) =>
                    patchProfile(profile.id, { permission }, true)
                  }
                />
              </div>
            </div>
          </div>
        ))}
      </div>

      <div
        className={"settings-card" + rowHighlight("settings-anchor-agentCliDefault")}
        id="settings-anchor-agentCliDefault"
      >
        <div className="settings-row">
          <div className="settings-row__text">
            <div className="settings-row__label">{t("settings.agentCli.default")}</div>
          </div>
          <Select
            className="agent-cli-default__select"
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
