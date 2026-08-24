/**
 * Sidebar rail of remote Grok sessions, grouped by SSH host.
 */
import { useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { Tip } from "@/components/ui/tooltip";
import type { MessageKey, Vars } from "@/i18n";
import type { Project } from "@/lib/app/sidebarModels";
import * as api from "@/lib/api";
import { nextSessionTitle } from "@/lib/sidebarSessionRename";
import {
  remainingRemoteCount,
  remotePathTip,
  remoteSessionLabel,
  remoteTitleKey,
} from "@/lib/sshRemoteSessionDisplay";
import { useSshWatch } from "@/providers/SshWatchProvider";

type TFn = (k: MessageKey, vars?: Vars) => string;

type Props = {
  t: TFn;
  newChat: (project?: Project | null) => void | Promise<void>;
};

export function SshRemoteSessionRail({ t, newChat }: Props) {
  const watch = useSshWatch();
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [openError, setOpenError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  if (watch.watchAliases.length === 0) return null;

  const untitled = t("sidebar.remoteUntitled");

  const commitRename = (alias: string, id: string, current: string) => {
    const next = nextSessionTitle(draft, current);
    setEditingKey(null);
    if (next) watch.renameRemoteSession(alias, id, next);
  };

  const openRow = async (alias: string, cwd: string, id: string) => {
    if (opening) return;
    setSelectedKey(remoteTitleKey(alias, id));
    setOpenError(null);
    watch.setDraftRemote({ alias, path: cwd });
    if (!cwd) {
      setOpenError(t("sidebar.remoteOpenFailed", { error: untitled }));
      return;
    }
    setOpening(true);
    try {
      const proj = (await api.projectAddSsh(alias, cwd, true)) as Project;
      await newChat(proj);
    } catch (e) {
      setOpenError(
        t("sidebar.remoteOpenFailed", { error: String(e) }),
      );
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="ssh-remote-rail">
      {watch.watchAliases.map((alias) => {
        const sessions = watch.sessionsByAlias[alias] ?? [];
        const total = watch.totalsByAlias[alias] ?? sessions.length;
        const remaining = remainingRemoteCount(total, sessions.length);
        return (
          <div key={alias} className="ssh-remote-rail__host">
            <div className="sidebar__section-label">
              {t("sidebar.remoteHost", { alias })}
            </div>
            {sessions.length === 0 ? (
              <div className="sidebar-empty__hint">
                {t("sidebar.remoteSessionsHint")}
              </div>
            ) : (
              sessions.map((s) => {
                const key = remoteTitleKey(alias, s.id);
                const label = remoteSessionLabel({
                  title: s.title,
                  cwd: s.cwd,
                  custom: watch.titleOverlay[key],
                  untitled,
                });
                const tip = remotePathTip(alias, s.cwd);
                const editing = editingKey === key;
                const active = selectedKey === key;
                return (
                  <Tip key={key} label={tip} placement="bottom">
                    <button
                      type="button"
                      className={
                        "ssh-remote-rail__row" +
                        (active ? " is-active" : "")
                      }
                      aria-label={label}
                      onClick={(e: MouseEvent<HTMLButtonElement>) => {
                        if (e.detail > 1) return;
                        if (editing) return;
                        void openRow(alias, s.cwd, s.id);
                      }}
                      onDoubleClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        setEditingKey(key);
                        setDraft(label);
                        window.setTimeout(() => inputRef.current?.select(), 0);
                      }}
                    >
                      {editing ? (
                        <input
                          ref={inputRef}
                          className="tree-l3__rename ssh-remote-rail__rename"
                          value={draft}
                          aria-label={t("session.renamePrompt")}
                          spellCheck={false}
                          autoComplete="off"
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) => setDraft(e.target.value)}
                          onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
                            e.stopPropagation();
                            if (e.nativeEvent.isComposing) return;
                            if (e.key === "Enter") {
                              e.preventDefault();
                              commitRename(alias, s.id, label);
                            } else if (e.key === "Escape") {
                              e.preventDefault();
                              setEditingKey(null);
                            }
                          }}
                          onBlur={() => commitRename(alias, s.id, label)}
                        />
                      ) : (
                        <span className="ssh-remote-rail__title">{label}</span>
                      )}
                    </button>
                  </Tip>
                );
              })
            )}
            {remaining > 0 ? (
              <div className="ssh-remote-rail__more">
                <div className="sidebar-empty__hint">
                  {t("sidebar.remoteRemaining", { n: remaining })}
                </div>
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  onClick={() => void watch.loadMore(alias)}
                >
                  {t("sidebar.remoteLoadMore")}
                </button>
              </div>
            ) : null}
          </div>
        );
      })}
      {openError ? (
        <div className="settings-row__hint is-danger" role="alert">
          {openError}
        </div>
      ) : null}
    </div>
  );
}
