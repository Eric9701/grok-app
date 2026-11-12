import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { GlassModal } from "@/components/GlassModal";
import { IconChevronRight, IconFileText, IconFolder } from "@/components/icons";
import { MarkdownBody } from "@/components/MarkdownBody";
import { createT, intlLocale, type Locale, type MessageKey } from "@/i18n";
import * as api from "@/lib/api";
import {
  ensureWikiCompileEvents,
  formatWikiElapsed,
  getWikiCompileTasks,
  startWikiCompile,
  subscribeWikiCompileTasks,
  wikiCompileActivity,
  type WikiCompileStage,
} from "./compileTasks";
import { formatWikiNotice } from "./errors";
import { buildWikiTree, groupWikiPages } from "./groups";
import type { WikiLibrary } from "./libraries";
import { WikiCreateDialog } from "./WikiCreateDialog";

function pageLabel(path: string) {
  return path.replace(/^wiki\//, "").replace(/\.md$/, "");
}

function pageTitle(path: string) {
  const name = path.split("/").pop() || path;
  return name.replace(/\.md$/, "");
}

function formatCount(locale: Locale, value: number) {
  return new Intl.NumberFormat(intlLocale(locale)).format(value);
}

const WIKI_CLI_STARTED_EVENT = "wiki://cli-started";

const STAGE_KEY: Record<WikiCompileStage, MessageKey> = {
  read: "kanban.wikiStage.read",
  analyze: "kanban.wikiStage.analyze",
  generate: "kanban.wikiStage.generate",
  write: "kanban.wikiStage.write",
};

export function WikiPage({
  locale,
  onOpenSettings,
  onToast,
}: {
  locale: Locale;
  onOpenSettings?: () => void;
  onToast?: (message: string | null) => void;
}) {
  const tr = useMemo(() => createT(locale), [locale]);
  const compileTasks = useSyncExternalStore(
    subscribeWikiCompileTasks,
    getWikiCompileTasks,
    getWikiCompileTasks,
  );
  const [clock, setClock] = useState(() => Date.now());
  useEffect(() => {
    ensureWikiCompileEvents();
  }, []);
  const [libraries, setLibraries] = useState<WikiLibrary[]>([]);
  const [ready, setReady] = useState(false);
  const usable = useMemo(
    () => libraries.filter((library) => library.path.trim()),
    [libraries],
  );
  const [projectPath, setProjectPath] = useState("");
  const runningCompile = compileTasks.find(
    (task) => task.status === "running" && task.projectPath === projectPath,
  );
  useEffect(() => {
    if (!runningCompile) return;
    const id = window.setInterval(() => setClock(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [runningCompile?.id]);
  const [pages, setPages] = useState<string[]>([]);
  const [hits, setHits] = useState<string[] | null>(null);
  const [selected, setSelected] = useState("");
  const [body, setBody] = useState("");
  const [query, setQuery] = useState("");
  const [issues, setIssues] = useState<string[]>([]);
  const [source, setSource] = useState("");
  const [sources, setSources] = useState<string[]>([]);
  const [dirs, setDirs] = useState<string[]>([]);
  const [ingested, setIngested] = useState<Record<string, boolean>>({});
  const [view, setView] = useState<"library" | "files" | "raw">("library");
  const [openDirs, setOpenDirs] = useState<Record<string, boolean>>({ raw: true, wiki: true });
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>({});
  const [createOpen, setCreateOpen] = useState(false);
  const [urlOpen, setUrlOpen] = useState(false);
  const [urlDraft, setUrlDraft] = useState("");
  const [deletePath, setDeletePath] = useState("");
  const [sourceTick, setSourceTick] = useState(0);
  const [models, setModels] = useState<Array<{ id: string; label: string }>>([]);
  const [modelId, setModelId] = useState("");
  const [seedModel, setSeedModel] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [note, setNote] = useState("");
  const compileStatus = (() => {
    if (!runningCompile) return "";
    const step =
      runningCompile.steps.find((item) => item.status === "in_progress") ??
      runningCompile.steps[0];
    if (!step) return "";
    if (wikiCompileActivity(runningCompile, clock) === "quiet") {
      return tr("wiki.compilingQuiet");
    }
    return tr("wiki.compilingLive", {
      stage: tr(STAGE_KEY[step.id]),
      time: formatWikiElapsed(clock - runningCompile.startedAt),
    });
  })();
  const statusText = compileStatus || note;
  const [error, setError] = useState("");
  const [recompileOpen, setRecompileOpen] = useState(false);
  const compileEpoch = useRef(0);
  const startedEpoch = useRef(0);

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
        setSeedModel(
          settings.wikiCompileModelId?.trim() || settings.modelId?.trim() || "",
        );
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
      setDirs([]);
      setSource("");
      return;
    }
    let stop = false;
    void Promise.all([api.wikiListRaw(projectPath), api.wikiListDirs(projectPath)])
      .then(async ([rows, folders]) => {
        if (stop) return;
        setSources(rows);
        setDirs(folders);
        setSource((current) => (rows.includes(current) ? current : (rows[0] ?? "")));
        const flags = await Promise.all(
          rows.map(async (path) => {
            const [fingerprint, cached] = await Promise.all([
              api.wikiFingerprint(projectPath, path).catch(() => ""),
              api.wikiCacheGet(projectPath, path).catch(() => null),
            ]);
            return [path, !!fingerprint && cached === fingerprint] as const;
          }),
        );
        if (!stop) setIngested(Object.fromEntries(flags));
      })
      .catch(() => {
        if (!stop) {
          setSources([]);
          setDirs([]);
          setSource("");
        }
      });
    return () => {
      stop = true;
    };
  }, [projectPath, sourceTick]);

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
    if (!projectPath || !api.isTauri()) return;
    let stop = false;
    let unlisten = () => {};
    void api
      .listen<string>(WIKI_CLI_STARTED_EVENT, (payload) => {
        if (stop || payload !== projectPath) return;
        const epoch = compileEpoch.current;
        if (epoch === 0 || startedEpoch.current === epoch) return;
        startedEpoch.current = epoch;
        const message = tr("wiki.started");
        setNote(message);
        onToast?.(message);
        window.setTimeout(() => onToast?.(null), 4200);
      })
      .then((fn) => {
        if (stop) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      stop = true;
      unlisten();
    };
  }, [projectPath, tr, onToast]);

  const visible = hits ?? pages;
  const compile = async (force = false, explicit?: string) => {
    if (!projectPath || busy) return;
    const sourcePath = (explicit ?? source).trim();
    if (explicit) setSource(explicit);
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
          setSource(sourcePath);
          setRecompileOpen(true);
          return;
        }
      } catch {
        /* A missing record just means this compile should run. */
      }
    }
    setRecompileOpen(false);
    compileEpoch.current += 1;
    setBusy(true);
    setError("");
    setNote("");
    try {
      const library = usable.find((item) => item.path === projectPath);
      const latest = await api.settingsGet().catch(() => null);
      const chosen =
        latest?.wikiCompileModelId?.trim() || modelId.trim() || undefined;
      const result = await startWikiCompile({
        projectPath,
        projectName: library?.name || projectPath,
        sourcePath,
        modelId: chosen,
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
        setSourceTick((tick) => tick + 1);
        if (result.written[0]) {
          setView("library");
          setSelected(result.written[0]);
        }
      }
    } catch (err) {
      setNote("");
      setError(formatWikiNotice(tr, err, sourcePath));
    } finally {
      setBusy(false);
    }
  };

  const reloadSources = () => setSourceTick((tick) => tick + 1);

  const importFiles = async () => {
    if (!projectPath || !api.isTauri()) return;
    const picked = await api.pickAttachFiles();
    if (!picked.length) return;
    try {
      await api.wikiImportFiles(projectPath, picked);
      reloadSources();
      setError("");
    } catch (err) {
      setError(formatWikiNotice(tr, err));
    }
  };

  const importFolder = async () => {
    if (!projectPath || !api.isTauri()) return;
    const picked = await api.pickDirectory();
    if (!picked) return;
    try {
      await api.wikiImportFiles(projectPath, [picked]);
      reloadSources();
      setError("");
    } catch (err) {
      setError(formatWikiNotice(tr, err));
    }
  };

  const importUrl = async () => {
    if (!projectPath || !urlDraft.trim()) return;
    try {
      await api.wikiImportUrl(projectPath, urlDraft.trim());
      setUrlDraft("");
      setUrlOpen(false);
      reloadSources();
      setError("");
    } catch (err) {
      setError(formatWikiNotice(tr, err));
    }
  };

  const removeRaw = async () => {
    if (!projectPath || !deletePath) return;
    try {
      await api.wikiRemoveRaw(projectPath, deletePath);
      if (selected === deletePath) setSelected("");
      setDeletePath("");
      reloadSources();
    } catch (err) {
      setError(formatWikiNotice(tr, err));
      setDeletePath("");
    }
  };

  const rawQuery = query.trim().toLowerCase();
  const visibleSources = rawQuery
    ? sources.filter((path) => path.toLowerCase().includes(rawQuery))
    : sources;
  const groups = groupWikiPages(visible);
  const tree = buildWikiTree(
    [...pages, ...sources],
    dirs,
  );

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
        <div className="wiki-empty-panel">
          <p>{tr("wiki.noLibraries")}</p>
          <div className="wiki-empty-panel__actions">
            <button type="button" className="btn btn--primary" onClick={() => setCreateOpen(true)}>
              {tr("wiki.create")}
            </button>
            {onOpenSettings ? (
              <button type="button" className="btn btn--ghost" onClick={onOpenSettings}>
                {tr("wiki.openSettings")}
              </button>
            ) : null}
          </div>
        </div>
        <WikiCreateDialog
          open={createOpen}
          onClose={() => setCreateOpen(false)}
          t={tr}
          onCreated={(libraries) => {
            setLibraries(libraries);
            setCreateOpen(false);
          }}
        />
      </div>
    );
  }

  const renderTree = (nodes: ReturnType<typeof buildWikiTree>, depth = 0): ReactNode => (
    <ul className={depth > 0 ? "wiki-pages wiki-pages--nested" : "wiki-pages"}>
      {nodes.map((node) => {
        const opened = openDirs[node.path] !== false;
        return (
          <li key={node.path}>
            {node.dir ? (
              <>
                <button
                  type="button"
                  className="wiki-tree__row"
                  aria-expanded={opened}
                  onClick={() =>
                    setOpenDirs((current) => ({
                      ...current,
                      [node.path]: current[node.path] === false,
                    }))
                  }
                >
                  <IconChevronRight
                    size={14}
                    className={"wiki-twist" + (opened ? " is-open" : "")}
                  />
                  <span className="wiki-tree__mark">
                    <IconFolder size={15} />
                  </span>
                  <span className="wiki-tree__name">{node.name}</span>
                </button>
                {opened ? renderTree(node.children, depth + 1) : null}
              </>
            ) : (
              <button
                type="button"
                className={"wiki-tree__row" + (node.path === selected ? " is-selected" : "")}
                aria-pressed={node.path === selected}
                onClick={() => setSelected(node.path)}
              >
                <span className="wiki-tree__file">
                  <IconFileText size={14} />
                </span>
                <span className="wiki-tree__name">{node.name}</span>
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
  const libraryName = usable.find((library) => library.path === projectPath)?.name ?? "";

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
        <div className="wiki-views" role="group" aria-label={tr("wiki.views")}>
          <button
            type="button"
            className={"wiki-chip" + (view === "library" ? " is-selected" : "")}
            aria-pressed={view === "library"}
            onClick={() => setView("library")}
          >
            {tr("wiki.viewLibrary")}
          </button>
          <button
            type="button"
            className={"wiki-chip" + (view === "files" ? " is-selected" : "")}
            aria-pressed={view === "files"}
            onClick={() => setView("files")}
          >
            {tr("wiki.viewFiles")}
          </button>
          <button
            type="button"
            className={"wiki-chip" + (view === "raw" ? " is-selected" : "")}
            aria-pressed={view === "raw"}
            onClick={() => setView("raw")}
          >
            {tr("wiki.kind.raw")}
          </button>
        </div>
        {libraryName ? <p className="wiki-library-name">{libraryName}</p> : null}
        <div className="wiki-pages-scroll">
          {view === "files" ? (
            tree.length === 0 ? (
              <p className="wiki-empty">{tr("wiki.pagesEmpty")}</p>
            ) : (
              renderTree(tree)
            )
          ) : view === "raw" ? (
            <p className="wiki-rail-note">{tr("wiki.rawPageHint")}</p>
          ) : loading && visible.length === 0 ? (
            <div className="wiki-skeleton" aria-label={tr("wiki.loading")}>
              <span />
              <span />
              <span />
              <span />
            </div>
          ) : (
            <>
              {groups.length === 0 ? (
                <p className="wiki-empty">
                  {query.trim() ? tr("wiki.searchEmpty") : tr("wiki.pagesEmpty")}
                </p>
              ) : (
                groups.map((group) => {
                  const opened = query.trim() ? true : openGroups[group.key] !== false;
                  const label = group.labelKey ? tr(group.labelKey as MessageKey) : group.label;
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
                        <span className={"wiki-kind__mark wiki-kind__mark--" + group.key}>
                          <IconFolder size={15} />
                        </span>
                        <span className="wiki-kind__label">{label}</span>
                        <span className="wiki-kind__count">{group.pages.length}</span>
                      </button>
                      {opened ? (
                        <ul className="wiki-pages wiki-kind__pages">
                          {group.pages.map((path) => (
                            <li key={path}>
                              <button
                                type="button"
                                className={"wiki-page-link" + (path === selected ? " is-selected" : "")}
                                aria-label={pageLabel(path)}
                                aria-pressed={path === selected}
                                onClick={() => setSelected(path)}
                              >
                                {pageTitle(path)}
                              </button>
                            </li>
                          ))}
                        </ul>
                      ) : null}
                    </section>
                  );
                })
              )}
            </>
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
          {view !== "raw" && statusText ? (
            <p className="wiki-note" role="status">
              {statusText}
            </p>
          ) : null}
          {view !== "raw" && error ? (
            <p className="wiki-error" role="alert">
              {error}
            </p>
          ) : null}
        </div>
      </aside>
      <article className="wiki-article">
        {view === "raw" ? (
          <div className="wiki-raw-page">
            <header className="wiki-raw-page__head">
              <p className="wiki-kicker">
                {tr("wiki.kind.raw")}
                <span className="wiki-kind__count">{visibleSources.length}</span>
              </p>
              <div className="wiki-raw-page__actions">
                <button type="button" className="btn btn--sm btn--ghost" disabled={busy} onClick={() => void importFiles()}>
                  {tr("wiki.importFiles")}
                </button>
                <button type="button" className="btn btn--sm btn--ghost" disabled={busy} onClick={() => void importFolder()}>
                  {tr("wiki.importFolder")}
                </button>
                <button type="button" className="btn btn--sm btn--ghost" disabled={busy} onClick={() => setUrlOpen(true)}>
                  {tr("wiki.importUrl")}
                </button>
              </div>
            </header>
            {statusText ? (
              <p className="wiki-note" role="status">
                {statusText}
              </p>
            ) : null}
            {error ? (
              <p className="wiki-error" role="alert">
                {error}
              </p>
            ) : null}
            {visibleSources.length === 0 ? (
              <div className="wiki-empty-article">
                <p>{tr("wiki.rawEmpty")}</p>
              </div>
            ) : (
              <ul className="wiki-raw-page__list">
                {visibleSources.map((path) => (
                  <li key={path} className={"wiki-raw-row" + (path === selected ? " is-selected" : "")}>
                    <button
                      type="button"
                      className={"wiki-page-link" + (path === selected ? " is-selected" : "")}
                      aria-pressed={path === selected}
                      onClick={() => setSelected(path)}
                    >
                      {path.replace(/^raw\//, "")}
                    </button>
                    <div className="wiki-raw-row__meta">
                      {ingested[path] ? <span className="wiki-ingested">{tr("wiki.ingested")}</span> : null}
                      <button
                        type="button"
                        className="btn btn--sm btn--primary"
                        disabled={busy}
                        onClick={() => void compile(false, path)}
                      >
                        {busy && source === path ? tr("wiki.compiling") : tr("wiki.extract")}
                      </button>
                      <button
                        type="button"
                        className="btn btn--sm btn--ghost"
                        disabled={busy}
                        aria-label={tr("wiki.rawDelete")}
                        onClick={() => setDeletePath(path)}
                      >
                        {tr("wiki.rawDelete")}
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            )}
            {selected.startsWith("raw/") && body ? (
              <div className="wiki-raw-preview">
                <MarkdownBody locale={locale}>{body}</MarkdownBody>
              </div>
            ) : null}
          </div>
        ) : selected && body ? (
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
        open={urlOpen}
        onClose={() => setUrlOpen(false)}
        title={tr("wiki.importUrlTitle")}
        size="sm"
        closeLabel={tr("common.close")}
        footer={
          <>
            <button type="button" className="btn btn--ghost" onClick={() => setUrlOpen(false)}>
              {tr("common.cancel")}
            </button>
            <button type="button" className="btn btn--primary" onClick={() => void importUrl()}>
              {tr("wiki.importUrl")}
            </button>
          </>
        }
      >
        <p>{tr("wiki.importUrlHint")}</p>
        <input
          className="settings-input"
          value={urlDraft}
          onChange={(event) => setUrlDraft(event.target.value)}
          spellCheck={false}
          aria-label={tr("wiki.importUrl")}
        />
      </GlassModal>
      <GlassModal
        open={!!deletePath}
        onClose={() => setDeletePath("")}
        title={tr("wiki.rawDeleteTitle")}
        size="sm"
        closeLabel={tr("common.close")}
        footer={
          <>
            <button type="button" className="btn btn--ghost" onClick={() => setDeletePath("")}>
              {tr("common.cancel")}
            </button>
            <button type="button" className="btn btn--primary" onClick={() => void removeRaw()}>
              {tr("wiki.rawDelete")}
            </button>
          </>
        }
      >
        <p>{tr("wiki.rawDeleteBody")}</p>
      </GlassModal>
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
