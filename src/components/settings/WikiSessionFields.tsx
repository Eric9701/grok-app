/**
 * Settings → 知识库配置: which agent sessions are collected, and the extraction prompts.
 */
import { useCallback, useEffect, useState } from "react";
import { Select } from "@/components/Select";
import { UiSwitch } from "@/components/settings/shared";
import type { Vars } from "@/i18n";
import * as api from "@/lib/api";
import type { WikiLibrary } from "@/wiki/libraries";
import { LOCKED_GENERATION_RULES } from "@/wiki/engine";
import type { WikiPrompts, WikiSessionConfig, WikiSessionSource } from "@/lib/api";

const RUNTIMES = ["atlas", "claude", "cursor", "codex"] as const;

type Tr = (key: string, vars?: Vars) => string;

function emptyConfig(): WikiSessionConfig {
  return {
    sources: RUNTIMES.map((runtime) => ({ runtime, enabled: true, home: "" })),
    cwdPrefixes: [],
    included: [],
    excluded: [],
    autoIncludeNew: false,
  };
}

function runtimeLabel(t: Tr, runtime: string) {
  if (runtime === "claude") return t("wiki.runtime.claude");
  if (runtime === "cursor") return t("wiki.runtime.cursor");
  if (runtime === "codex") return t("wiki.runtime.codex");
  return t("wiki.runtime.atlas");
}

export function WikiSessionFields({
  t,
  rowHighlight,
}: {
  t: Tr;
  rowHighlight: (anchorId: string) => string;
}) {
  const [libraries, setLibraries] = useState<WikiLibrary[]>([]);
  const [project, setProject] = useState("");
  const [config, setConfig] = useState<WikiSessionConfig>(emptyConfig());
  const [prompts, setPrompts] = useState<WikiPrompts>({ digest: "", analyze: "", generate: "" });
  const [prefixes, setPrefixes] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const load = useCallback(async (path: string) => {
    if (!path || !api.isTauri()) return;
    const [nextConfig, nextPrompts] = await Promise.all([
      api.wikiSessionConfigGet(path),
      api.wikiPromptsGet(path),
    ]);
    setConfig(nextConfig);
    setPrompts(nextPrompts);
    setPrefixes(nextConfig.cwdPrefixes.join("\n"));
  }, []);

  useEffect(() => {
    if (!api.isTauri()) return;
    let stop = false;
    void api.settingsGet().then((settings) => {
      if (stop) return;
      const rows = (settings.wikiLibraries ?? []).filter((library) => library.path.trim());
      setLibraries(rows);
      const first = rows.find((library) => library.agentDefault)?.path || rows[0]?.path || "";
      setProject(first);
    }).catch(() => {
      if (!stop) setError(t("wiki.pathInvalid"));
    });
    return () => {
      stop = true;
    };
  }, [t]);

  useEffect(() => {
    if (!project) return;
    void load(project).catch(() => setError(t("wiki.errProject")));
  }, [load, project, t]);

  const persistConfig = async (next: WikiSessionConfig) => {
    if (!project || saving) return;
    setSaving(true);
    setError("");
    try {
      const saved = await api.wikiSessionConfigSet(project, next);
      setConfig(saved);
      setPrefixes(saved.cwdPrefixes.join("\n"));
    } catch (err) {
      setError(err instanceof Error ? err.message : t("wiki.pathInvalid"));
    } finally {
      setSaving(false);
    }
  };

  const persistPrompts = async (next: WikiPrompts) => {
    if (!project || saving) return;
    setSaving(true);
    setError("");
    try {
      const saved = await api.wikiPromptsSet(project, next);
      setPrompts(saved);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("wiki.pathInvalid"));
    } finally {
      setSaving(false);
    }
  };

  const updateSource = (runtime: string, patch: Partial<WikiSessionSource>) => {
    const sources = config.sources.map((source) =>
      source.runtime === runtime ? { ...source, ...patch } : source,
    );
    const next = { ...config, sources };
    setConfig(next);
    void persistConfig(next);
  };

  return (
    <>
      <div
        className={"settings-card" + rowHighlight("settings-anchor-wikiSessions")}
        id="settings-anchor-wikiSessions"
      >
        <div className="settings-row settings-row--stack">
          <div className="settings-row__text">
            <div className="settings-row__label">{t("wiki.sessionSources")}</div>
            <div className="settings-row__desc">{t("wiki.sessionSourcesDesc")}</div>
          </div>
          {libraries.length === 0 ? (
            <p className="wiki-settings-empty">{t("wiki.settingsEmpty")}</p>
          ) : (
            <>
              {libraries.length > 1 ? (
                <Select
                  value={project}
                  options={libraries.map((library) => ({
                    value: library.path,
                    label: library.name || library.path,
                  }))}
                  onChange={setProject}
                  aria-label={t("wiki.libraryPick")}
                />
              ) : null}
              {config.sources.map((source) => (
                <div key={source.runtime} className="wiki-session-source">
                  <UiSwitch
                    checked={source.enabled}
                    disabled={saving}
                    label={runtimeLabel(t, source.runtime)}
                    onChange={(enabled) => updateSource(source.runtime, { enabled })}
                  />
                  <span className="settings-row__label">{runtimeLabel(t, source.runtime)}</span>
                  <input
                    className="settings-input"
                    value={source.home}
                    disabled={saving}
                    aria-label={t("wiki.sessionHome")}
                    placeholder={t("wiki.sessionHome")}
                    onChange={(event) => {
                      const home = event.target.value;
                      setConfig((current) => ({
                        ...current,
                        sources: current.sources.map((item) =>
                          item.runtime === source.runtime ? { ...item, home } : item,
                        ),
                      }));
                    }}
                    onBlur={(event) => {
                      const home = event.target.value;
                      const next = {
                        ...config,
                        sources: config.sources.map((item) =>
                          item.runtime === source.runtime ? { ...item, home } : item,
                        ),
                      };
                      setConfig(next);
                      void persistConfig(next);
                    }}
                  />
                </div>
              ))}
              <label className="settings-row__label" htmlFor="wiki-session-prefixes">
                {t("wiki.sessionPrefixes")}
              </label>
              <p className="settings-row__desc">{t("wiki.sessionPrefixesDesc")}</p>
              <textarea
                id="wiki-session-prefixes"
                className="settings-input wiki-prompt"
                value={prefixes}
                disabled={saving}
                onChange={(event) => setPrefixes(event.target.value)}
                onBlur={(event) => {
                  const cwdPrefixes = event.target.value
                    .split(/\r?\n/)
                    .map((line) => line.trim())
                    .filter(Boolean);
                  setPrefixes(cwdPrefixes.join("\n"));
                  void persistConfig({ ...config, cwdPrefixes });
                }}
              />
            </>
          )}
          {error ? <p className="settings-row__desc" role="alert">{error}</p> : null}
        </div>
      </div>
      <div
        className={"settings-card" + rowHighlight("settings-anchor-wikiPrompts")}
        id="settings-anchor-wikiPrompts"
      >
        <div className="settings-row settings-row--stack">
          <div className="settings-row__text">
            <div className="settings-row__label">{t("wiki.promptsTitle")}</div>
            <div className="settings-row__desc">{t("wiki.promptsDesc")}</div>
          </div>
          {(
            [
              ["digest", "wiki.promptDigest"],
              ["analyze", "wiki.promptAnalyze"],
              ["generate", "wiki.promptGenerate"],
            ] as const
          ).map(([key, label]) => (
            <label key={key} className="wiki-prompt-field">
              <span className="settings-row__label">{t(label)}</span>
              <textarea
                className="settings-input wiki-prompt"
                value={prompts[key]}
                disabled={saving || !project}
                aria-label={t(label)}
                onChange={(event) =>
                  setPrompts((current) => ({ ...current, [key]: event.target.value }))
                }
                onBlur={(event) => {
                  const next = { ...prompts, [key]: event.target.value };
                  setPrompts(next);
                  void persistPrompts(next);
                }}
              />
            </label>
          ))}
          <button
            type="button"
            className="btn btn--sm btn--ghost"
            disabled={saving || !project}
            onClick={() => void persistPrompts({ digest: "", analyze: "", generate: "" })}
          >
            {t("wiki.promptRestore")}
          </button>
          <p className="settings-row__label">{t("wiki.promptLocked")}</p>
          <pre className="wiki-prompt-lock">{LOCKED_GENERATION_RULES}</pre>
        </div>
      </div>
    </>
  );
}
