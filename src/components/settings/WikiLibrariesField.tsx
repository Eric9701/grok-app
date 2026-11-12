/**
 * Settings → 知识库配置: personal knowledge-base directories.
 * One row can be the agent default. The knowledge page lists these rows.
 */
import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { Vars } from "@/i18n";
import { WikiCreateDialog } from "@/wiki/WikiCreateDialog";
import { WikiInitDialog } from "@/wiki/WikiInitDialog";
import {
  displayLibraryPath,
  isAbsoluteLibraryPath,
  libraryFolderName,
  wikiPathKey,
  type WikiLibrary,
} from "@/wiki/libraries";

type Props = {
  t: (k: string, vars?: Vars) => string;
  rowHighlight: (anchorId: string) => string;
};

export function WikiLibrariesField({ t, rowHighlight }: Props) {
  const [rows, setRows] = useState<WikiLibrary[]>([]);
  const [ready, setReady] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [initTarget, setInitTarget] = useState<WikiLibrary | null>(null);
  const [note, setNote] = useState("");

  const refresh = useCallback(async () => {
    if (!api.isTauri()) {
      setReady(true);
      return;
    }
    try {
      const settings = await api.settingsGet();
      const loaded = (settings.wikiLibraries ?? []).map((row) => ({
        ...row,
        path: displayLibraryPath(row.path),
      }));
      setRows(loaded);
      const dirty = (settings.wikiLibraries ?? []).some(
        (row, index) => row.path !== loaded[index]?.path,
      );
      if (dirty) {
        const saved = await api.settingsSet({ ...settings, wikiLibraries: loaded });
        setRows(
          (saved.wikiLibraries ?? loaded).map((row) => ({
            ...row,
            path: displayLibraryPath(row.path),
          })),
        );
      }
      setError("");
    } catch (err) {
      setError(err instanceof Error ? err.message : t("wiki.pathInvalid"));
    } finally {
      setReady(true);
    }
  }, [t]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const persist = async (next: WikiLibrary[]) => {
    if (!api.isTauri() || saving) return;
    setSaving(true);
    setError("");
    try {
      const settings = await api.settingsGet();
      const saved = await api.settingsSet({ ...settings, wikiLibraries: next });
      setRows(
        (saved.wikiLibraries ?? []).map((row) => ({
          ...row,
          path: displayLibraryPath(row.path),
        })),
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : t("wiki.pathInvalid"));
    } finally {
      setSaving(false);
    }
  };

  const add = async () => {
    if (!api.isTauri() || saving) return;
    const pickedRaw = await api.pickDirectory();
    const picked = pickedRaw ? displayLibraryPath(pickedRaw) : "";
    if (!picked) return;
    if (!isAbsoluteLibraryPath(picked)) {
      setError(t("wiki.pathInvalid"));
      return;
    }
    if (rows.some((row) => wikiPathKey(row.path) === wikiPathKey(picked))) {
      setError(t("wiki.duplicate"));
      return;
    }
    const next = [
      ...rows,
      {
        id: globalThis.crypto?.randomUUID?.() ?? `wiki-${Date.now()}`,
        name: libraryFolderName(picked),
        path: picked,
        agentDefault: rows.every((row) => !row.agentDefault),
      },
    ];
    await persist(next);
  };

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-wikiLibraries")}
      id="settings-anchor-wikiLibraries"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("wiki.settingsTitle")}</div>
          <div className="settings-row__desc">{t("wiki.settingsDesc")}</div>
        </div>
        <div className="wiki-lib__actions">
          <button
            type="button"
            className="btn btn--primary"
            disabled={saving || !api.isTauri()}
            onClick={() => setCreateOpen(true)}
          >
            {t("wiki.create")}
          </button>
          <button
            type="button"
            className="btn btn--ghost"
            disabled={saving || !api.isTauri()}
            onClick={() => void add()}
          >
            {t("wiki.addDirectory")}
          </button>
        </div>
        <WikiCreateDialog
          open={createOpen}
          onClose={() => setCreateOpen(false)}
          t={t}
          onCreated={(libraries) => {
            setRows(libraries);
            setCreateOpen(false);
          }}
        />
        {ready && rows.length === 0 ? (
          <p className="wiki-settings-empty">{t("wiki.settingsEmpty")}</p>
        ) : (
          <ul className="wiki-libraries">
            {rows.map((row) => (
              <li key={row.id} className="wiki-lib">
                <label className="wiki-lib__name">
                  <span className="wiki-kicker">{t("wiki.name")}</span>
                  <input
                    className="settings-input"
                    value={row.name}
                    disabled={saving}
                    aria-label={t("wiki.name")}
                    onChange={(event) => {
                      const name = event.target.value;
                      setRows((current) =>
                        current.map((item) =>
                          item.id === row.id ? { ...item, name } : item,
                        ),
                      );
                    }}
                    onBlur={(event) => {
                      const name =
                        event.target.value.trim() || libraryFolderName(row.path);
                      void persist(
                        rows.map((item) =>
                          item.id === row.id ? { ...item, name } : item,
                        ),
                      );
                    }}
                  />
                </label>
                <div className="wiki-lib__actions">
                  <button
                    type="button"
                    className="btn btn--sm btn--ghost"
                    disabled={saving}
                    onClick={() => {
                      setNote("");
                      setInitTarget(row);
                    }}
                  >
                    {t("wiki.init")}
                  </button>
                  <button
                    type="button"
                    className={
                      "btn btn--sm" + (row.agentDefault ? " btn--primary" : " btn--ghost")
                    }
                    aria-pressed={!!row.agentDefault}
                    disabled={saving || !!row.agentDefault}
                    onClick={() =>
                      void persist(
                        rows.map((item) => ({
                          ...item,
                          agentDefault: item.id === row.id,
                        })),
                      )
                    }
                  >
                    {row.agentDefault ? t("wiki.agentDefault") : t("wiki.makeDefault")}
                  </button>
                  <button
                    type="button"
                    className="btn btn--sm btn--ghost"
                    disabled={saving}
                    onClick={() => {
                      const rest = rows.filter((item) => item.id !== row.id);
                      const next = rest.some((item) => item.agentDefault)
                        ? rest
                        : rest.map((item, index) => ({
                            ...item,
                            agentDefault: index === 0,
                          }));
                      void persist(next);
                    }}
                  >
                    {t("wiki.remove")}
                  </button>
                </div>
                <p className="wiki-lib__path" title={displayLibraryPath(row.path)}>
                  {displayLibraryPath(row.path)}
                </p>
              </li>
            ))}
          </ul>
        )}
        <WikiInitDialog
          library={initTarget}
          onClose={() => setInitTarget(null)}
          onDone={() => setNote(t("wiki.initDone"))}
          t={t}
        />
        {note ? (
          <p className="wiki-settings-empty" role="status">
            {note}
          </p>
        ) : null}
        {error ? (
          <p className="wiki-error" role="alert">
            {error}
          </p>
        ) : null}
      </div>
    </div>
  );
}
