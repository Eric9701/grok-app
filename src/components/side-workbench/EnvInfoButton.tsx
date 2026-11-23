/**
 * Main chrome `env` control — environment info dropdown by default.
 * A header pin switches to the chat-column dock (park with the right rail).
 * Phase 3: display collected git/session status + jump callbacks; no full git write ops.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { createT, type Locale } from "@/i18n";
import {
  IconBrandGithub,
  IconChevronDown,
  IconClose,
  IconDeviceDesktop,
  IconEnv,
  IconFileDiff,
  IconGitBranch,
  IconGitCommit,
  IconPin,
  IconPinOff,
} from "@/components/icons";
import { Tip } from "@/components/ui/tooltip";
import * as api from "@/lib/api";
import {
  loadEnvInfoPinnedPref,
  saveEnvInfoPinnedPref,
} from "@/lib/envInfoPinnedPref";
import { useFloatingMenu } from "@/lib/floatingMenu";
import { OPEN_PRESENCE_MS, useOpenPresence } from "@/lib/openPresence";
import { envChangesBadge } from "@/lib/envChangesBadge";
import { pathBaseName } from "@/lib/sessionChanges";
import { envReviewJumpEnabled } from "@/lib/sideWorkbench";
import { EnvCommitPushDialog } from "./EnvCommitPushDialog";

export type EnvInfoJump =
  | { type: "review" }
  | { type: "local" }
  | { type: "branch" }
  | { type: "push" }
  | { type: "pr" };

/** Optional session/workspace line stats passed from the workbench. */
export type EnvChangeSummary = {
  add: number;
  del: number;
  /** Distinct files when line stats are unavailable. */
  fileCount?: number;
};

export type EnvInfoButtonProps = {
  locale: Locale | string;
  projectPath?: string | null;
  /** Short project label for the Local row (falls back to path basename). */
  projectName?: string | null;
  isGitProject?: boolean;
  /** Optional +N/−M (or file count) from session/workspace summary. */
  changeSummary?: EnvChangeSummary | null;
  /** Optional branch override; when omitted, loaded from git status on open. */
  branch?: string | null;
  /** Right rail open — parks the dock instead of closing it. */
  asideOpen?: boolean;
  className?: string;
  onJump?: (jump: EnvInfoJump) => void;
  onToast?: (message: string, ms?: number) => void;
  /** Refresh git dirty state after a commit or push. */
  onGitChanged?: () => void;
};

type EnvSnapshot = {
  branch: string | null;
  dirtyCount: number;
  isGit: boolean;
  /** Git status for `loadedPath` has finished. */
  gitLoaded: boolean;
  loadedPath: string;
  prLabel: string | null;
  prChecking: boolean;
};

function emptySnapshot(isGit: boolean): EnvSnapshot {
  return {
    branch: null,
    dirtyCount: 0,
    isGit,
    gitLoaded: false,
    loadedPath: "",
    prLabel: null,
    prChecking: false,
  };
}

export function EnvInfoButton({
  locale,
  projectPath = null,
  projectName = null,
  isGitProject = false,
  changeSummary = null,
  branch: branchProp = null,
  asideOpen = false,
  className = "",
  onJump,
  onToast,
  onGitChanged,
}: EnvInfoButtonProps) {
  const tr = useMemo(() => createT(locale as Locale), [locale]);
  const [open, setOpen] = useState(false);
  const [commitOpen, setCommitOpen] = useState(false);
  const [pinned, setPinned] = useState(loadEnvInfoPinnedPref);
  const [parked, setParked] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const loadGen = useRef(0);
  const asideOpenRef = useRef(asideOpen);
  const pinnedRef = useRef(pinned);
  pinnedRef.current = pinned;
  const [host, setHost] = useState<HTMLElement | null>(null);

  const [snap, setSnap] = useState<EnvSnapshot>(() =>
    emptySnapshot(isGitProject),
  );

  const presence = useOpenPresence(
    open,
    true,
    pinned ? 320 : OPEN_PRESENCE_MS,
  );
  const { pos, style, settled } = useFloatingMenu({
    open: presence.mounted && !pinned,
    triggerRef,
    panelRef,
    onClose: () => {
      if (!pinnedRef.current) setOpen(false);
    },
    placement: "down",
    // Chrome trailing control: hang panel so right edges match the env icon.
    align: "end",
    fitContent: true,
    minWidth: 280,
    estHeight: 320,
    gap: 6,
  });
  const entered = pinned
    ? presence.entered
    : Boolean(presence.entered && settled);

  const persistPinned = useCallback((next: boolean) => {
    setPinned(next);
    saveEnvInfoPinnedPref(next);
    if (!next) setParked(false);
    if (next) setOpen(true);
  }, []);

  useLayoutEffect(() => {
    const el = document.querySelector(".main");
    setHost(el instanceof HTMLElement ? el : null);
  }, []);

  useLayoutEffect(() => {
    const wasOpen = asideOpenRef.current;
    asideOpenRef.current = asideOpen;
    if (!open || !pinned) {
      setParked(false);
      return;
    }
    if (!wasOpen && asideOpen) setParked(true);
    if (wasOpen && !asideOpen) setParked(false);
  }, [asideOpen, open, pinned]);

  useEffect(() => {
    if (!presence.mounted || !pinned) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [presence.mounted, pinned]);

  const localLabel = useMemo(() => {
    const named = (projectName || "").trim();
    if (named) return named;
    const path = (projectPath || "").trim();
    if (!path) return "";
    return pathBaseName(path) || path;
  }, [projectName, projectPath]);

  const loadSnapshot = useCallback(async () => {
    const path = (projectPath || "").trim();
    const gen = ++loadGen.current;
    if (!path || !api.isTauri()) {
      setSnap(emptySnapshot(isGitProject));
      return;
    }

    setSnap((prev) => ({
      ...prev,
      isGit: isGitProject,
      gitLoaded: prev.loadedPath === path ? prev.gitLoaded : false,
      prChecking: isGitProject,
      prLabel: null,
      branch: branchProp?.trim() || prev.branch,
    }));

    let branch: string | null = branchProp?.trim() || null;
    let dirtyCount = 0;
    let isGit = isGitProject;

    try {
      const status = await api.gitStatus(path);
      if (gen !== loadGen.current) return;
      isGit = !!status?.available;
      branch = (status?.branch || "").trim() || branch;
      dirtyCount = Array.isArray(status?.files) ? status.files.length : 0;
    } catch {
      if (gen !== loadGen.current) return;
      isGit = false;
    }

    setSnap({
      branch,
      dirtyCount,
      isGit,
      gitLoaded: true,
      loadedPath: path,
      prLabel: null,
      prChecking: isGit,
    });

    if (!isGit) return;

    try {
      const prs = await api.gitPrList(path, { limit: 30, state: "open" });
      if (gen !== loadGen.current) return;
      if (!prs?.available || !prs.ghFound) {
        setSnap((prev) => ({
          ...prev,
          prChecking: false,
          prLabel: tr("side.env.prUnavailable"),
        }));
        return;
      }
      const list = Array.isArray(prs.prs) ? prs.prs : [];
      const head = (branch || "").trim();
      const match = head
        ? list.find(
            (p) => (p.headRefName || "").trim() === head,
          )
        : null;
      const first = match || list[0] || null;
      if (first) {
        const n = first.number;
        const title = (first.title || "").trim();
        const label = title
          ? tr("side.env.prOpen", { n: String(n), title })
          : tr("side.env.prOpenNumber", { n: String(n) });
        setSnap((prev) => ({
          ...prev,
          prChecking: false,
          prLabel: label,
        }));
      } else {
        setSnap((prev) => ({
          ...prev,
          prChecking: false,
          prLabel: tr("side.env.prNone"),
        }));
      }
    } catch {
      if (gen !== loadGen.current) return;
      setSnap((prev) => ({
        ...prev,
        prChecking: false,
        prLabel: tr("side.env.prUnavailable"),
      }));
    }
  }, [branchProp, isGitProject, projectPath, tr]);

  useEffect(() => {
    if (!open) return;
    void loadSnapshot();
  }, [open, loadSnapshot]);

  const branchLabel =
    (branchProp || "").trim() ||
    (snap.branch || "").trim() ||
    tr("side.env.branch");

  const changesRight = useMemo((): ReactNode => {
    const path = (projectPath || "").trim();
    const badge = envChangesBadge({
      gitLoaded: snap.gitLoaded && snap.loadedPath === path,
      isGit: snap.isGit,
      dirtyCount: snap.dirtyCount,
      sessionAdd: changeSummary?.add,
      sessionDel: changeSummary?.del,
      sessionFileCount: changeSummary?.fileCount,
    });
    if (badge.kind === "delta") {
      return (
        <span className="sw-env-menu__delta" data-testid="env-changes-delta">
          <span className="sw-env-menu__add">+{badge.add}</span>
          <span className="sw-env-menu__del">−{badge.del}</span>
        </span>
      );
    }
    if (badge.kind === "files") {
      return (
        <span className="sw-env-menu__right-text" data-testid="env-changes-files">
          {tr("changes.count", { n: String(badge.count) })}
        </span>
      );
    }
    return null;
  }, [
    changeSummary,
    projectPath,
    snap.dirtyCount,
    snap.gitLoaded,
    snap.isGit,
    snap.loadedPath,
    tr,
  ]);

  const gitReady = snap.isGit || isGitProject;
  const chevron = (
    <IconChevronDown size={14} className="sw-env-menu__chev" aria-hidden />
  );

  const row = (
    key: string,
    icon: ReactNode,
    label: string,
    right: ReactNode,
    jump: EnvInfoJump | null,
    enabled: boolean,
  ) => (
    <button
      key={key}
      type="button"
      className="sw-env-menu__row"
      disabled={!enabled || !jump}
      data-testid={`env-row-${key}`}
      onClick={() => {
        if (!jump || !enabled) return;
        if (!pinned) setOpen(false);
        if (jump.type === "push") setCommitOpen(true);
        onJump?.(jump);
      }}
    >
      <span className="sw-env-menu__icon" aria-hidden>
        {icon}
      </span>
      <span className="sw-env-menu__label" title={label}>
        {label}
      </span>
      <span className="sw-env-menu__right">{right}</span>
    </button>
  );

  const panel = (
    <div
      ref={panelRef}
      className={
        pinned
          ? "sw-env-menu" +
            (entered ? " is-open" : "") +
            (parked ? " is-parked" : "")
          : "sw-env-menu menu-panel" + (entered ? " is-open" : "")
      }
      style={pinned ? undefined : style}
      role="dialog"
      aria-label={tr("side.envTitle")}
      data-testid="env-info-menu"
      data-env-mode={pinned ? "dock" : "dropdown"}
    >
      <div className="sw-env-menu__head">
        <span className="sw-env-menu__title">{tr("side.envTitle")}</span>
        <div className="sw-env-menu__actions">
          <button
            type="button"
            className={
              "chrome-btn sw-env-menu__pin" + (pinned ? " is-on" : "")
            }
            aria-label={pinned ? tr("side.env.unpin") : tr("side.env.pin")}
            aria-pressed={pinned}
            data-testid="env-info-pin"
            onClick={(e) => {
              e.stopPropagation();
              persistPinned(!pinned);
            }}
          >
            {pinned ? <IconPinOff size={14} /> : <IconPin size={14} />}
          </button>
          {pinned ? (
            <button
              type="button"
              className="chrome-btn sw-env-menu__close"
              aria-label={tr("common.close")}
              data-testid="env-info-close"
              onClick={() => setOpen(false)}
            >
              <IconClose size={14} />
            </button>
          ) : null}
        </div>
      </div>
      <div className="sw-env-menu__body">
        {row(
          "changes",
          <IconFileDiff size={16} />,
          tr("side.env.changes"),
          changesRight,
          envReviewJumpEnabled(gitReady) ? { type: "review" } : null,
          envReviewJumpEnabled(gitReady),
        )}
        {row(
          "local",
          <IconDeviceDesktop size={16} />,
          tr("side.env.local"),
          <>
            {localLabel ? (
              <span
                className="sw-env-menu__right-text"
                title={projectPath || localLabel}
                data-testid="env-local-name"
              >
                {localLabel}
              </span>
            ) : null}
            {chevron}
          </>,
          { type: "local" },
          true,
        )}
        {row(
          "branch",
          <IconGitBranch size={16} />,
          branchLabel,
          chevron,
          gitReady ? { type: "branch" } : null,
          gitReady,
        )}
        {row(
          "push",
          <IconGitCommit size={16} />,
          tr("side.env.push"),
          null,
          gitReady ? { type: "push" } : null,
          gitReady,
        )}
        {row(
          "pr",
          <IconBrandGithub size={16} />,
          snap.prChecking
            ? tr("side.env.prChecking")
            : snap.prLabel || tr("side.env.prChecking"),
          null,
          gitReady ? { type: "pr" } : null,
          gitReady,
        )}
      </div>
    </div>
  );

  const overlay =
    presence.mounted && (pinned ? host : pos)
      ? createPortal(
          pinned ? (
            <div className="sw-env-dock" aria-hidden={parked || !entered}>
              {panel}
            </div>
          ) : (
            panel
          ),
          pinned && host ? host : document.body,
        )
      : null;

  return (
    <>
      <Tip label={tr("side.env")}>
        <button
          ref={triggerRef}
          type="button"
          className={
            "chrome-btn main__pane-toggle sw-env-btn" +
            (open ? " is-on" : "") +
            (className ? ` ${className}` : "")
          }
          aria-label={tr("side.env")}
          aria-expanded={open}
          data-testid="env-info-button"
          onClick={() => setOpen((v) => !v)}
        >
          <IconEnv size={16} />
        </button>
      </Tip>
      {overlay}
      <EnvCommitPushDialog
        open={commitOpen}
        locale={locale}
        projectPath={projectPath}
        branchHint={(branchProp || snap.branch || "").trim() || null}
        onClose={() => setCommitOpen(false)}
        onToast={onToast}
        onDone={() => {
          onGitChanged?.();
          const path = (projectPath || "").trim();
          setSnap((prev) =>
            prev.isGit
              ? {
                  ...prev,
                  dirtyCount: 0,
                  gitLoaded: true,
                  loadedPath: path || prev.loadedPath,
                }
              : prev,
          );
          void loadSnapshot();
        }}
      />
    </>
  );
}
