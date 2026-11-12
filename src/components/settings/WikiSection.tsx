/**
 * Settings → 知识库配置. Directories plus the compile turn cap.
 */
import { useCallback, useEffect, useState } from "react";
import { Select } from "@/components/Select";
import * as api from "@/lib/api";
import type { WikiUsageEntry } from "@/lib/api";
import { intlLocale, type Vars } from "@/i18n";
import { useSettingsModel } from "@/providers/SettingsModelContext";
import { WikiLibrariesField } from "./WikiLibrariesField";

export const DEFAULT_WIKI_COMPILE_TURNS = 6;

export function clampWikiCompileTurns(raw: number): number {
  if (!Number.isFinite(raw)) return DEFAULT_WIKI_COMPILE_TURNS;
  const rounded = Math.round(raw);
  if (rounded < 1 || rounded > 20) return DEFAULT_WIKI_COMPILE_TURNS;
  return rounded;
}

function WikiCompileTurnsField({
  t,
  rowHighlight,
}: {
  t: (key: string, vars?: Vars) => string;
  rowHighlight: (anchorId: string) => string;
}) {
  const [turns, setTurns] = useState(String(DEFAULT_WIKI_COMPILE_TURNS));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!api.isTauri()) return;
    let stop = false;
    void api.settingsGet().then((settings) => {
      if (stop) return;
      setTurns(String(clampWikiCompileTurns(settings.wikiCompileMaxTurns ?? DEFAULT_WIKI_COMPILE_TURNS)));
    }).catch(() => {
      if (!stop) setError(t("wiki.pathInvalid"));
    });
    return () => {
      stop = true;
    };
  }, [t]);

  const persist = useCallback(async (next: number) => {
    if (!api.isTauri() || saving) return;
    setSaving(true);
    setError("");
    try {
      const settings = await api.settingsGet();
      const saved = await api.settingsSet({ ...settings, wikiCompileMaxTurns: next });
      setTurns(String(clampWikiCompileTurns(saved.wikiCompileMaxTurns ?? next)));
    } catch (err) {
      setError(err instanceof Error ? err.message : t("wiki.pathInvalid"));
    } finally {
      setSaving(false);
    }
  }, [saving, t]);

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-wikiCompileTurns")}
      id="settings-anchor-wikiCompileTurns"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("wiki.compileTurns")}</div>
          <div className="settings-row__desc">{t("wiki.compileTurnsDesc")}</div>
        </div>
        <input
          className="settings-input wiki-turns"
          type="number"
          min={1}
          max={20}
          step={1}
          value={turns}
          disabled={saving || !api.isTauri()}
          aria-label={t("wiki.compileTurns")}
          onChange={(event) => setTurns(event.target.value)}
          onBlur={() => {
            const next = clampWikiCompileTurns(Number(turns));
            setTurns(String(next));
            void persist(next);
          }}
        />
        {error ? (
          <p className="settings-row__desc" role="alert">
            {error}
          </p>
        ) : null}
      </div>
    </div>
  );
}

function WikiCompileModelField({
  t,
  rowHighlight,
}: {
  t: (key: string, vars?: Vars) => string;
  rowHighlight: (anchorId: string) => string;
}) {
  const [models, setModels] = useState<Array<{ id: string; label: string }>>([]);
  const [modelId, setModelId] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!api.isTauri()) return;
    let stop = false;
    void Promise.all([api.modelsListAvailable(), api.settingsGet()])
      .then(([catalog, settings]) => {
        if (stop) return;
        const options = catalog.models.map((model) => ({
          id: model.id,
          label: model.label || model.id,
        }));
        setModels(options);
        const saved = settings.wikiCompileModelId?.trim() ?? "";
        const fallback = settings.modelId?.trim() ?? catalog.defaultModelId ?? "";
        const chosen = options.some((model) => model.id === saved)
          ? saved
          : options.some((model) => model.id === fallback)
            ? fallback
            : (options[0]?.id ?? "");
        setModelId(chosen);
      })
      .catch(() => {
        if (!stop) setError(t("wiki.modelsEmpty"));
      });
    return () => {
      stop = true;
    };
  }, [t]);

  const persist = async (next: string) => {
    if (!api.isTauri() || saving || !next) return;
    setSaving(true);
    setError("");
    try {
      const settings = await api.settingsGet();
      const saved = await api.settingsSet({ ...settings, wikiCompileModelId: next });
      setModelId(saved.wikiCompileModelId?.trim() || next);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("wiki.pathInvalid"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-wikiCompileModel")}
      id="settings-anchor-wikiCompileModel"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("wiki.model")}</div>
          <div className="settings-row__desc">{t("wiki.modelHint")}</div>
        </div>
        {models.length === 0 ? (
          <p className="settings-row__desc">{t("wiki.modelsEmpty")}</p>
        ) : (
          <Select
            className="wiki-compile-model"
            value={modelId}
            options={models.map((model) => ({ value: model.id, label: model.label }))}
            onChange={(next) => {
              setModelId(next);
              void persist(next);
            }}
            disabled={saving}
            aria-label={t("wiki.model")}
          />
        )}
        {error ? (
          <p className="settings-row__desc" role="alert">
            {error}
          </p>
        ) : null}
      </div>
    </div>
  );
}

type HistoryRow = WikiUsageEntry & { library: string };

function WikiCompileHistory() {
  const { t, rowHighlight, locale, formatSessionWhen } = useSettingsModel();
  const [rows, setRows] = useState<HistoryRow[]>([]);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    if (!api.isTauri()) {
      setReady(true);
      return;
    }
    let stop = false;
    void api
      .settingsGet()
      .then(async (settings) => {
        const libraries = (settings.wikiLibraries ?? []).filter((library) => library.path.trim());
        const lists = await Promise.all(
          libraries.map(async (library) => {
            const items = await api.wikiUsageList(library.path).catch(() => []);
            return items.map((item) => ({
              ...item,
              library: library.name.trim() || library.path,
            }));
          }),
        );
        if (stop) return;
        setRows(lists.flat().sort((a, b) => (a.at < b.at ? 1 : a.at > b.at ? -1 : 0)));
      })
      .catch(() => {
        if (!stop) setRows([]);
      })
      .finally(() => {
        if (!stop) setReady(true);
      });
    return () => {
      stop = true;
    };
  }, []);

  const count = (value: number) => new Intl.NumberFormat(intlLocale(locale)).format(value);

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-wikiHistory")}
      id="settings-anchor-wikiHistory"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("wiki.history")}</div>
          <div className="settings-row__desc">{t("wiki.historyDesc")}</div>
        </div>
        {ready && rows.length === 0 ? (
          <p className="wiki-settings-empty">{t("wiki.historyEmpty")}</p>
        ) : (
          <ul className="wiki-history">
            {rows.map((row) => (
              <li key={`${row.library}-${row.at}-${row.source}`}>
                <span className="wiki-history__when">{formatSessionWhen(row.at, locale)}</span>
                <span className="wiki-history__source" title={row.source}>
                  {row.library}
                  {" · "}
                  {row.source.replace(/^raw\//, "")}
                </span>
                <span className="wiki-history__meta">
                  {row.reported
                    ? `${t("wiki.usageIn", { count: count(row.inputTokens) })} · ${t("wiki.usageOut", { count: count(row.outputTokens) })} · ${t("wiki.usageTotal", { count: count(row.totalTokens) })}`
                    : t("wiki.usageUnreported")}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

export function WikiSection() {
  const { t, rowHighlight } = useSettingsModel();
  return (
    <>
      <WikiLibrariesField t={t} rowHighlight={rowHighlight} />
      <WikiCompileModelField t={t} rowHighlight={rowHighlight} />
      <WikiCompileTurnsField t={t} rowHighlight={rowHighlight} />
      <WikiCompileHistory />
    </>
  );
}
