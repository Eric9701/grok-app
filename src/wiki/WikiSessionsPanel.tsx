import { useCallback, useEffect, useMemo, useState } from "react";
import { IconChevronRight } from "@/components/icons";
import { formatSessionWhen, UiCheck, UiSwitch } from "@/components/settings/shared";
import type { Locale, MessageKey } from "@/i18n";
import * as api from "@/lib/api";
import type { WikiSessionConfig, WikiSessionHit } from "@/lib/api";

type Tr = (key: MessageKey, vars?: Record<string, string | number>) => string;

const RUNTIME_ORDER = ["atlas", "claude", "cursor", "codex"];

function runtimeLabel(tr: Tr, runtime: string) {
  if (runtime === "atlas") return tr("wiki.runtime.atlas");
  if (runtime === "claude") return tr("wiki.runtime.claude");
  if (runtime === "cursor") return tr("wiki.runtime.cursor");
  if (runtime === "codex") return tr("wiki.runtime.codex");
  return runtime;
}

function groupSessions(hits: WikiSessionHit[]) {
  const buckets = new Map<string, WikiSessionHit[]>();
  for (const hit of hits) {
    const key = hit.runtime || "atlas";
    const list = buckets.get(key);
    if (list) list.push(hit);
    else buckets.set(key, [hit]);
  }
  const known = RUNTIME_ORDER.filter((key) => buckets.has(key));
  const rest = [...buckets.keys()].filter((key) => !RUNTIME_ORDER.includes(key));
  return [...known, ...rest].map((key) => ({
    key,
    hits: buckets.get(key) ?? [],
  }));
}

export function WikiSessionsPanel({
  projectPath,
  busy,
  locale,
  tr,
  onExtract,
}: {
  projectPath: string;
  busy: boolean;
  locale: Locale;
  tr: Tr;
  onExtract: (paths: string[]) => Promise<void>;
}) {
  const [hits, setHits] = useState<WikiSessionHit[]>([]);
  const [config, setConfig] = useState<WikiSessionConfig | null>(null);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState("");
  const [note, setNote] = useState("");
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>({});
  const groups = useMemo(() => groupSessions(hits), [hits]);

  const scan = useCallback(async () => {
    if (!projectPath || !api.isTauri()) return;
    setScanning(true);
    setError("");
    try {
      const [nextConfig, nextHits] = await Promise.all([
        api.wikiSessionConfigGet(projectPath),
        api.wikiSessionsScan(projectPath),
      ]);
      setConfig(nextConfig);
      setHits(nextHits);
    } catch (err) {
      setError(err instanceof Error ? err.message : tr("wiki.errProject"));
    } finally {
      setScanning(false);
    }
  }, [projectPath, tr]);

  useEffect(() => {
    void scan();
  }, [scan]);

  const save = async (next: WikiSessionConfig) => {
    const saved = await api.wikiSessionConfigSet(projectPath, next);
    setConfig(saved);
    return saved;
  };

  const toggle = async (hit: WikiSessionHit, on: boolean) => {
    if (!config) return;
    const same = (item: { runtime: string; id: string }) =>
      item.runtime === hit.runtime && item.id === hit.id;
    const next: WikiSessionConfig = {
      ...config,
      included: on
        ? [...config.included.filter((item) => !same(item)), { runtime: hit.runtime, id: hit.id }]
        : config.included.filter((item) => !same(item)),
      excluded: on
        ? config.excluded.filter((item) => !same(item))
        : [...config.excluded.filter((item) => !same(item)), { runtime: hit.runtime, id: hit.id }],
    };
    setHits((current) =>
      current.map((item) => (same(item) ? { ...item, included: on } : item)),
    );
    try {
      await save(next);
      setError("");
    } catch (err) {
      setError(err instanceof Error ? err.message : tr("wiki.pathInvalid"));
    }
  };

  const sync = async () => {
    if (!projectPath || busy) return;
    setError("");
    setNote("");
    try {
      const result = await api.wikiSessionsSync(projectPath);
      if (result.rollups.length === 0) {
        setNote(tr("wiki.sessionsNone"));
        return;
      }
      setNote(tr("wiki.sessionsSyncDone", { count: result.rollups.length }));
      await onExtract(result.rollups);
    } catch (err) {
      setError(err instanceof Error ? err.message : tr("wiki.errProject"));
    }
  };

  return (
    <div className="wiki-raw-page">
      <header className="wiki-raw-page__head">
        <p className="wiki-kicker">
          {tr("wiki.sessionsTitle")}
          <span className="wiki-kind__count">{hits.length}</span>
        </p>
        <div className="wiki-raw-page__actions">
          <button type="button" className="btn btn--sm btn--ghost" disabled={scanning || busy} onClick={() => void scan()}>
            {tr("wiki.sessionsScan")}
          </button>
          <button type="button" className="btn btn--sm btn--primary" disabled={scanning || busy} onClick={() => void sync()}>
            {tr("wiki.sessionsSync")}
          </button>
        </div>
      </header>
      {config ? (
        <div className="wiki-session-bar">
          <div className="wiki-session-auto">
            <UiSwitch
              checked={config.autoIncludeNew}
              disabled={busy}
              label={tr("wiki.autoInclude")}
              onChange={(next) => {
                void save({ ...config, autoIncludeNew: next }).catch((err: unknown) => {
                  setError(err instanceof Error ? err.message : tr("wiki.pathInvalid"));
                });
              }}
            />
            <span>{tr("wiki.autoInclude")}</span>
          </div>
          <p className="wiki-session-hint">{tr("wiki.autoIncludeDesc")}</p>
        </div>
      ) : null}
      {note ? <p className="wiki-note" role="status">{note}</p> : null}
      {error ? <p className="wiki-error" role="alert">{error}</p> : null}
      {hits.length === 0 ? (
        <div className="wiki-empty-article">
          <p>{tr("wiki.sessionsEmpty")}</p>
        </div>
      ) : (
        <div className="wiki-session-groups">
          {groups.map((group) => {
            const opened = openGroups[group.key] !== false;
            return (
              <section key={group.key} className="wiki-kind">
                <button
                  type="button"
                  className="wiki-kind__head"
                  aria-expanded={opened}
                  onClick={() =>
                    setOpenGroups((current) => ({
                      ...current,
                      [group.key]: current[group.key] === false,
                    }))
                  }
                >
                  <IconChevronRight
                    size={14}
                    className={"wiki-twist" + (opened ? " is-open" : "")}
                  />
                  <span className="wiki-kind__label">{runtimeLabel(tr, group.key)}</span>
                  <span className="wiki-kind__count">{group.hits.length}</span>
                </button>
                {opened ? (
                  <ul className="wiki-raw-page__list wiki-kind__pages">
                    {group.hits.map((hit) => (
                      <li key={`${hit.runtime}:${hit.id}`} className="wiki-raw-row wiki-session-row">
                        <UiCheck
                          checked={hit.included}
                          disabled={busy}
                          ariaLabel={hit.title}
                          onChange={(on) => void toggle(hit, on)}
                          label={
                            <span className="wiki-session-label">
                              <span className="wiki-session-main">
                                <span className="wiki-session-title">{hit.title}</span>
                                {hit.updatedAt ? (
                                  <time className="wiki-session-when" dateTime={hit.updatedAt}>
                                    {formatSessionWhen(hit.updatedAt, locale)}
                                  </time>
                                ) : null}
                              </span>
                              {hit.cwd ? <span className="wiki-session-cwd">{hit.cwd}</span> : null}
                            </span>
                          }
                        />
                      </li>
                    ))}
                  </ul>
                ) : null}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}
