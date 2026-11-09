import { useEffect, useMemo, useState } from "react";
import { GlassModal } from "@/components/GlassModal";
import { MarkdownBody } from "@/components/MarkdownBody";
import { Select } from "@/components/Select";
import { createT, intlLocale, type Locale } from "@/i18n";
import * as api from "@/lib/api";
import type { WikiUsageEntry } from "@/lib/api";
import { startWikiCompile } from "./compileTasks";
import { formatWikiNotice } from "./errors";
import type { WikiLibrary } from "./libraries";

function pageLabel(path: string) {
  return path.replace(/^wiki\//, "").replace(/\.md$/, "");
}

function formatCount(locale: Locale, value: number) {
  return new Intl.NumberFormat(intlLocale(locale)).format(value);
}

function formatWhen(locale: Locale, at: string) {
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return at;
  return new Intl.DateTimeFormat(intlLocale(locale), {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function WikiPage({
  locale,
  onOpenSettings,
}: {
  locale: Locale;
  onOpenSettings?: () => void;
}) {
  const tr = useMemo(() => createT(locale), [locale]);
  const [libraries, setLibraries] = useState<WikiLibrary[]>([]);
  const [ready, setReady] = useState(false);
  const usable = useMemo(
    () => libraries.filter((library) => library.path.trim()),
    [libraries],
  );
  const [projectPath, setProjectPath] = useState("");
  const [pages, setPages] = useState<string[]>([]);
  const [hits, setHits] = useState<string[] | null>(null);
  const [selected, setSelected] = useState("");
  const [body, setBody] = useState("");
  const [query, setQuery] = useState("");
  const [issues, setIssues] = useState<string[]>([]);
  const [source, setSource] = useState("");
  const [sources, setSources] = useState<string[]>([]);
  const [models, setModels] = useState<Array<{ id: string; label: string }>>([]);
  const [modelId, setModelId] = useState("");
  const [seedModel, setSeedModel] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const [recompileOpen, setRecompileOpen] = useState(false);
  const [usageRows, setUsageRows] = useState<WikiUsageEntry[]>([]);

  useEffect(() => {
    if (!api.isTauri()) {
      setReady(true);
      return;
    }
    let stop = false;
    void api
      .settingsGet()
      .then((settings) => {
        if (stop) return;
        setLibraries(settings.wikiLibraries ?? []);
        setSeedModel(settings.modelId?.trim() ?? "");
      })
      .catch(() => {
        if (!stop) setLibraries([]);
      })
      .finally(() => {
        if (!stop) setReady(true);
      });
    return () => {
      stop = true;
    };
  }, []);

  useEffect(() => {
    if (!api.isTauri()) return;
    let stop = false;
    void api
      .modelsListAvailable()
      .then((result) => {
        if (stop) return;
        setModels(
          result.models.map((model) => ({
            id: model.id,
            label: model.label || model.id,
          })),
        );
      })
      .catch(() => {
        if (!stop) setModels([]);
      });
    return () => {
      stop = true;
    };
  }, []);

  useEffect(() => {
    if (models.length === 0) return;
    setModelId((current) => {
      if (current && models.some((model) => model.id === current)) return current;
      if (seedModel && models.some((model) => model.id === seedModel)) return seedModel;
      return models[0]?.id ?? "";
    });
  }, [models, seedModel]);

  useEffect(() => {
    if (!ready) return;
    if (usable.some((library) => library.path === projectPath)) return;
    const preferred = usable.find((library) => library.agentDefault) ?? usable[0];
    setProjectPath(preferred?.path ?? "");
    setSelected("");
    setQuery("");
  }, [ready, usable, projectPath]);

  useEffect(() => {
    if (!projectPath || !api.isTauri()) {
      setSources([]);
      setSource("");
      return;
    }
    let stop = false;
    void api
      .wikiListRaw(projectPath)
      .then((rows) => {
        if (stop) return;
        setSources(rows);
        setSource((current) => (rows.includes(current) ? current : (rows[0] ?? "")));
      })
      .catch(() => {
        if (!stop) {
          setSources([]);
          setSource("");
        }
      });
    return () => {
      stop = true;
    };
  }, [projectPath]);

  useEffect(() => {
    if (!projectPath || !api.isTauri()) {
      setPages([]);
      setIssues([]);
      setLoading(false);
      return;
    }
    let stop = false;
    setLoading(true);
    void (async () => {
      try {
        const [listed, lint] = await Promise.all([
          api.wikiList(projectPath),
          api.wikiLint(projectPath),
        ]);
        if (stop) return;
        setPages(listed);
        setIssues([
          ...lint.broken.map((item) => tr("wiki.broken", { target: item.target })),
          ...lint.orphans.map((path) =>
            tr("wiki.orphan", { target: pageLabel(path) }),
          ),
        ]);
        setError("");
      } catch (err) {
        if (!stop) setError(formatWikiNotice(tr, err));
      } finally {
        if (!stop) setLoading(false);
      }
    })();
    return () => {
      stop = true;
    };
  }, [projectPath, tr]);

  useEffect(() => {
    const q = query.trim();
    if (!projectPath || !q || !api.isTauri()) {
      setHits(null);
      return;
    }
    let stop = false;
    const timer = window.setTimeout(() => {
      void api
        .wikiSearch(projectPath, q, 20)
        .then((found) => {
          if (!stop) setHits(found.map((hit) => hit.path));
        })
        .catch(() => {
          if (!stop) setHits([]);
        });
    }, 200);
    return () => {
      stop = true;
      window.clearTimeout(timer);
    };
  }, [projectPath, query]);

  useEffect(() => {
    if (!selected || !projectPath || !api.isTauri()) {
      setBody("");
      return;
    }
    let stop = false;
    void api
      .wikiRead(projectPath, selected)
      .then((text) => {
        if (!stop) setBody(text.content);
      })
      .catch(() => {
        if (!stop) setBody("");
      });
    return () => {
      stop = true;
    };
  }, [projectPath, selected]);

  useEffect(() => {
    if (!projectPath || !api.isTauri()) {
      setUsageRows([]);
      return;
    }
    let stop = false;
    void api
      .wikiUsageList(projectPath)
      .then((rows) => {
        if (!stop) setUsageRows(rows);
      })
      .catch(() => {
        if (!stop) setUsageRows([]);
      });
    return () => {
      stop = true;
    };
  }, [projectPath]);

  const visible = hits ?? pages;
  const usageTotal = usageRows.reduce(
    (sum, row) => sum + (row.reported ? row.totalTokens : 0),
    0,
  );
  const compile = async (force = false) => {
    if (!projectPath || busy) return;
    const sourcePath = source.trim();
    if (!sourcePath) {
      setNote("");
      setError(tr("wiki.errPath"));
      return;
    }
    if (!force) {
      try {
        const [fingerprint, cached] = await Promise.all([
          api.wikiFingerprint(projectPath, sourcePath),
          api.wikiCacheGet(projectPath, sourcePath),
        ]);
        if (cached && cached === fingerprint) {
          setRecompileOpen(true);
          return;
        }
      } catch {
        /* A missing record just means this compile should run. */
      }
    }
    setRecompileOpen(false);
    setBusy(true);
    setError("");
    setNote("");
    try {
      const library = usable.find((item) => item.path === projectPath);
      const result = await startWikiCompile({
        projectPath,
        projectName: library?.name || projectPath,
        sourcePath,
        modelId: modelId.trim() || undefined,
        force,
      });
      if (result.skipped) {
        setNote(tr("wiki.skipped"));
      } else {
        const tokens = result.usage?.totalTokens ?? 0;
        setNote(
          result.usage
            ? tr("wiki.compiledUsage", {
                count: result.written.length,
                tokens: formatCount(locale, tokens),
              })
            : tr("wiki.compiled", { count: result.written.length }),
        );
        const listed = await api.wikiList(projectPath);
        setPages(listed);
        if (result.written[0]) setSelected(result.written[0]);
        const rows = await api.wikiUsageList(projectPath).catch(() => []);
        setUsageRows(rows);
      }
    } catch (err) {
      setError(formatWikiNotice(tr, err, sourcePath));
    } finally {
      setBusy(false);
    }
  };

  if (!ready) {
    return (
      <div className="wiki-page wiki-page--empty" aria-busy="true">
        <div className="wiki-skeleton" aria-label={tr("wiki.loading")}>
          <span />
          <span />
          <span />
        </div>
      </div>
    );
  }

  if (usable.length === 0) {
    return (
      <div className="wiki-page wiki-page--empty">
        <p>{tr("wiki.noLibraries")}</p>
        {onOpenSettings ? (
          <button type="button" className="btn btn--primary" onClick={onOpenSettings}>
            {tr("wiki.openSettings")}
          </button>
        ) : null}
      </div>
    );
  }

  return (
    <div className="wiki-page">
      <aside className="wiki-rail" aria-label={tr("wiki.pages")}>
        {usable.length > 1 ? (
          <div className="wiki-projects" role="group" aria-label={tr("wiki.projects")}>
            {usable.map((project) => (
              <button
                key={project.id}
                type="button"
                className={
                  "wiki-chip" + (project.path === projectPath ? " is-selected" : "")
                }
                aria-pressed={project.path === projectPath}
                onClick={() => {
                  setProjectPath(project.path);
                  setSelected("");
                  setQuery("");
                }}
              >
                {project.name}
              </button>
            ))}
          </div>
        ) : null}
        <label className="wiki-search">
          <span className="wiki-kicker">{tr("wiki.search")}</span>
          <input
            className="settings-input"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            spellCheck={false}
          />
        </label>
        <div className="wiki-pages-scroll">
          {loading && visible.length === 0 ? (
            <div className="wiki-skeleton" aria-label={tr("wiki.loading")}>
              <span />
              <span />
              <span />
              <span />
            </div>
          ) : visible.length === 0 ? (
            <p className="wiki-empty">
              {query.trim() ? tr("wiki.searchEmpty") : tr("wiki.pagesEmpty")}
            </p>
          ) : (
            <ul className="wiki-pages">
              {visible.map((path) => (
                <li key={path}>
                  <button
                    type="button"
                    className={"wiki-page-link" + (path === selected ? " is-selected" : "")}
                    aria-pressed={path === selected}
                    onClick={() => setSelected(path)}
                  >
                    {pageLabel(path)}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="wiki-rail__foot">
          {issues.length === 0 ? (
            <p className="wiki-lint">{tr("wiki.lintClean")}</p>
          ) : (
            <ul className="wiki-issues">
              {issues.slice(0, 4).map((issue) => (
                <li key={issue}>{issue}</li>
              ))}
            </ul>
          )}
          <form
            className="wiki-compile"
            onSubmit={(event) => {
              event.preventDefault();
              void compile();
            }}
          >
            <label className="wiki-model">
              <span className="wiki-kicker">{tr("wiki.model")}</span>
              <Select
                value={modelId}
                options={models.map((model) => ({
                  value: model.id,
                  label: model.label,
                }))}
                onChange={setModelId}
                disabled={busy || models.length === 0}
                aria-label={tr("wiki.model")}
              />
              <span className="wiki-hint">
                {models.length === 0 ? tr("wiki.modelsEmpty") : tr("wiki.modelHint")}
              </span>
            </label>
            <label className="wiki-model">
              <span className="wiki-kicker">{tr("wiki.source")}</span>
              <Select
                value={source}
                options={sources.map((path) => ({
                  value: path,
                  label: path.replace(/^raw\//, ""),
                }))}
                onChange={setSource}
                disabled={busy || sources.length === 0}
                aria-label={tr("wiki.source")}
              />
              <span className="wiki-hint">
                {sources.length === 0 ? tr("wiki.rawEmpty") : tr("wiki.sourceHint")}
              </span>
            </label>
            <button
              type="submit"
              className="btn btn--primary"
              disabled={busy || sources.length === 0}
            >
              {busy ? tr("wiki.compiling") : tr("wiki.compile")}
            </button>
          </form>
          {note ? <p className="wiki-note">{note}</p> : null}
          <section className="wiki-usage" aria-label={tr("wiki.usage")}>
            <p className="wiki-kicker">{tr("wiki.usage")}</p>
            {usageRows.length === 0 ? (
              <p className="wiki-empty">{tr("wiki.usageEmpty")}</p>
            ) : (
              <>
                <p className="wiki-usage__total">
                  {tr("wiki.usageTotal", { count: formatCount(locale, usageTotal) })}
                </p>
                <ul className="wiki-usage__list">
                  {usageRows.slice(0, 6).map((row) => (
                    <li key={`${row.at}-${row.source}`}>
                      <span>{formatWhen(locale, row.at)}</span>
                      <span className="wiki-usage__source">{pageLabel(row.source)}</span>
                      {row.reported ? (
                        <span>
                          {tr("wiki.usageIn", { count: formatCount(locale, row.inputTokens) })}
                          {" · "}
                          {tr("wiki.usageOut", { count: formatCount(locale, row.outputTokens) })}
                          {" · "}
                          {tr("wiki.usageTotal", { count: formatCount(locale, row.totalTokens) })}
                        </span>
                      ) : (
                        <span>{tr("wiki.usageUnreported")}</span>
                      )}
                    </li>
                  ))}
                </ul>
              </>
            )}
          </section>
          {error ? (
            <p className="wiki-error" role="alert">
              {error}
            </p>
          ) : null}
        </div>
      </aside>
      <article className="wiki-article">
        {selected && body ? (
          <>
            <p className="wiki-kicker wiki-article__path">{pageLabel(selected)}</p>
            <MarkdownBody locale={locale}>{body}</MarkdownBody>
          </>
        ) : (
          <div className="wiki-empty-article">
            <p>{tr("wiki.articleEmpty")}</p>
            <p className="wiki-empty-article__hint">{tr("wiki.articleHint")}</p>
          </div>
        )}
      </article>
      <GlassModal
        open={recompileOpen}
        onClose={() => setRecompileOpen(false)}
        title={tr("wiki.recompileTitle")}
        size="sm"
        closeLabel={tr("common.close")}
        footer={
          <>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => setRecompileOpen(false)}
            >
              {tr("common.cancel")}
            </button>
            <button type="button" className="btn btn--primary" onClick={() => void compile(true)}>
              {tr("wiki.recompileConfirm")}
            </button>
          </>
        }
      >
        <p>{tr("wiki.recompileBody")}</p>
      </GlassModal>
    </div>
  );
}
