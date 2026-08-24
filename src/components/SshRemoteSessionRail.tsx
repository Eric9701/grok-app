/**
 * Sidebar remote sessions: host → cwd folder → same tree-l3 row as local chats.
 */
import {
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { SidebarSessionName } from "@/components/SidebarSessionName";
import { SidebarSessionRelativeTime } from "@/components/SidebarSessionRelativeTime";
import { SidebarTreeReveal } from "@/components/SidebarTreeReveal";
import { IconFolder } from "@/components/icons";
import { Spinner } from "@/components/ui/spinner";
import type { Locale, MessageKey, Vars } from "@/i18n";
import * as api from "@/lib/api";
import { nextSessionTitle } from "@/lib/sidebarSessionRename";
import {
  groupRemoteSessionsByCwd,
  remainingRemoteCount,
  remotePathTip,
  remoteSessionLabel,
  remoteTitleKey,
} from "@/lib/sshRemoteSessionDisplay";
import { useSshWatch } from "@/providers/SshWatchProvider";

type TFn = (k: MessageKey, vars?: Vars) => string;

type Props = {
  t: TFn;
  locale: Locale;
  showRelativeTime: boolean;
  onOpenSession: (sessionId: string) => void;
};

function pathFoldKey(alias: string, cwd: string): string {
  return `${alias}\t${cwd}`;
}

export function SshRemoteSessionRail({
  t,
  locale,
  showRelativeTime,
  onOpenSession,
}: Props) {
  const watch = useSshWatch();
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [openError, setOpenError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [hostOpen, setHostOpen] = useState<Record<string, boolean>>({});
  const [pathOpen, setPathOpen] = useState<Record<string, boolean>>({});
  const inputRef = useRef<HTMLInputElement>(null);

  if (watch.watchAliases.length === 0) return null;

  const untitled = t("sidebar.remoteUntitled");

  const commitRename = (alias: string, id: string, current: string) => {
    const next = nextSessionTitle(draft, current);
    setEditingKey(null);
    if (next) watch.renameRemoteSession(alias, id, next);
  };

  const openRow = async (
    alias: string,
    cwd: string,
    id: string,
    label: string,
  ) => {
    if (opening) return;
    setSelectedKey(remoteTitleKey(alias, id));
    setOpenError(null);
    watch.setDraftRemote({ alias, path: cwd || alias });
    setOpening(true);
    try {
      const r = await api.sshOpenSession(alias, id, {
        cwd: cwd || null,
        titleHint: label,
      });
      if (!r.ok || !r.appSessionId) {
        setOpenError(
          t("sidebar.remoteOpenFailed", {
            error: r.error || untitled,
          }),
        );
        return;
      }
      onOpenSession(r.appSessionId);
    } catch (e) {
      setOpenError(t("sidebar.remoteOpenFailed", { error: String(e) }));
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
        const groups = groupRemoteSessionsByCwd(sessions);
        const hostExpanded = hostOpen[alias] !== false;
        return (
          <div key={alias} className="tree-project">
            <div
              className="tree-l2"
              role="button"
              tabIndex={0}
              aria-expanded={hostExpanded}
              onClick={() =>
                setHostOpen((prev) => ({
                  ...prev,
                  [alias]: !(prev[alias] !== false),
                }))
              }
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  setHostOpen((prev) => ({
                    ...prev,
                    [alias]: !(prev[alias] !== false),
                  }));
                }
              }}
            >
              <span className="tree-l2__icon" aria-hidden>
                <IconFolder size={15} />
              </span>
              <span className="tree-l2__name">
                {t("sidebar.remoteHost", { alias })}
              </span>
            </div>
            <SidebarTreeReveal open={hostExpanded}>
              <div className="tree-l3-list-wrap">
                {sessions.length === 0 ? (
                  <div className="sidebar-empty" style={{ padding: "4px 10px" }}>
                    {t("sidebar.remoteSessionsHint")}
                  </div>
                ) : (
                  groups.map((group) => {
                    const foldKey = pathFoldKey(alias, group.cwd);
                    const pathExpanded = pathOpen[foldKey] !== false;
                    const pathAria = group.cwd || group.label || untitled;
                    return (
                      <div
                        key={foldKey}
                        className="tree-project tree-project--remote-path"
                      >
                        <div
                          className="tree-l2 tree-l2--nested"
                          role="button"
                          tabIndex={0}
                          title={remotePathTip(alias, group.cwd)}
                          aria-expanded={pathExpanded}
                          aria-label={pathAria}
                          onClick={() =>
                            setPathOpen((prev) => ({
                              ...prev,
                              [foldKey]: !(prev[foldKey] !== false),
                            }))
                          }
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              setPathOpen((prev) => ({
                                ...prev,
                                [foldKey]: !(prev[foldKey] !== false),
                              }));
                            }
                          }}
                        >
                          <span className="tree-l2__icon" aria-hidden>
                            <IconFolder size={15} />
                          </span>
                          <span className="tree-l2__name">{group.label}</span>
                        </div>
                        <SidebarTreeReveal open={pathExpanded}>
                          <div className="tree-l3-list">
                            {group.sessions.map((s) => {
                              const key = remoteTitleKey(alias, s.id);
                              const label = remoteSessionLabel({
                                title: s.title,
                                cwd: s.cwd,
                                custom: watch.titleOverlay[key],
                                untitled,
                              });
                              const editing = editingKey === key;
                              const active = selectedKey === key;
                              const busy = opening && active;
                              return (
                                <div
                                  key={key}
                                  className={
                                    "tree-l3 tree-l3--nested" +
                                    (active ? " tree-l3--active" : "") +
                                    (busy ? " tree-l3--working" : "") +
                                    (editing ? " tree-l3--renaming" : "")
                                  }
                                  role="button"
                                  tabIndex={0}
                                  title={remotePathTip(alias, s.cwd)}
                                  aria-busy={busy || undefined}
                                  aria-label={label}
                                  onClick={(e: MouseEvent<HTMLDivElement>) => {
                                    if (e.detail > 1) return;
                                    if (editing) return;
                                    void openRow(alias, s.cwd, s.id, label);
                                  }}
                                  onDoubleClick={(e) => {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    setEditingKey(key);
                                    setDraft(label);
                                    window.setTimeout(
                                      () => inputRef.current?.select(),
                                      0,
                                    );
                                  }}
                                  onKeyDown={(
                                    e: KeyboardEvent<HTMLDivElement>,
                                  ) => {
                                    if (editing) return;
                                    if (e.key === "Enter" || e.key === " ") {
                                      e.preventDefault();
                                      void openRow(alias, s.cwd, s.id, label);
                                    }
                                    if (e.key === "F2") {
                                      e.preventDefault();
                                      setEditingKey(key);
                                      setDraft(label);
                                    }
                                  }}
                                >
                                  <span className="tree-l3__title">
                                    {editing ? (
                                      <input
                                        ref={inputRef}
                                        className="tree-l3__rename"
                                        value={draft}
                                        aria-label={t("session.renamePrompt")}
                                        spellCheck={false}
                                        autoComplete="off"
                                        onClick={(e) => e.stopPropagation()}
                                        onChange={(e) =>
                                          setDraft(e.target.value)
                                        }
                                        onKeyDown={(e) => {
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
                                        onBlur={() =>
                                          commitRename(alias, s.id, label)
                                        }
                                      />
                                    ) : (
                                      <SidebarSessionName title={label} />
                                    )}
                                  </span>
                                  {busy ? (
                                    <span
                                      className="tree-l3__status"
                                      aria-label={t("sidebar.remoteOpening")}
                                    >
                                      <Spinner
                                        size={14}
                                        className="tree-l3__spinner"
                                      />
                                    </span>
                                  ) : (
                                    <SidebarSessionRelativeTime
                                      updatedAt={s.updatedAt ?? undefined}
                                      locale={locale}
                                      enabled={showRelativeTime}
                                    />
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        </SidebarTreeReveal>
                      </div>
                    );
                  })
                )}
                {openError && selectedKey?.startsWith(`${alias}:`) ? (
                  <div className="tree-l3 tree-l3--hint" role="alert">
                    {openError}
                  </div>
                ) : null}
                {remaining > 0 ? (
                  <button
                    type="button"
                    className="tree-l3"
                    onClick={() => void watch.loadMore(alias)}
                  >
                    {t("sidebar.remoteLoadMore")}
                    <span className="tree-l3__time">
                      {t("sidebar.remoteRemaining", { n: remaining })}
                    </span>
                  </button>
                ) : null}
              </div>
            </SidebarTreeReveal>
          </div>
        );
      })}
    </div>
  );
}
