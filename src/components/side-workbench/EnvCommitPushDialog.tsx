/**
 * Env menu “Commit or push”.
 * Commits every worktree change, then pushes the current branch to origin.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { GlassModal } from "@/components/GlassModal";
import { createT, type Locale } from "@/i18n";
import * as api from "@/lib/api";
import {
  commitFailureKind,
  commitPushMode,
  commitPushNeedsMessage,
  pushOutputIsUpToDate,
} from "@/lib/envCommitPush";
import { redactShipOutput } from "@/lib/wtShipFlow";

type Props = {
  open: boolean;
  locale: Locale | string;
  projectPath?: string | null;
  branchHint?: string | null;
  onClose: () => void;
  onToast?: (message: string, ms?: number) => void;
  onDone?: () => void;
};

export function EnvCommitPushDialog({
  open,
  locale,
  projectPath,
  branchHint,
  onClose,
  onToast,
  onDone,
}: Props) {
  const tr = useMemo(() => createT(locale as Locale), [locale]);
  const branchHintRef = useRef(branchHint);
  branchHintRef.current = branchHint;
  const [fileCount, setFileCount] = useState(0);
  const [names, setNames] = useState<string[]>([]);
  const [branch, setBranch] = useState<string | null>(branchHint ?? null);
  const [detached, setDetached] = useState(false);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState("");
  const [phase, setPhase] = useState<"idle" | "committing" | "pushing">("idle");
  const [error, setError] = useState<string | null>(null);
  const [didCommit, setDidCommit] = useState(false);

  useEffect(() => {
    if (!open) return;
    setMessage("");
    setError(null);
    setPhase("idle");
    setDidCommit(false);
    setFileCount(0);
    setNames([]);
    setDetached(false);
    setBranch(branchHintRef.current ?? null);
    const path = projectPath?.trim();
    if (!path || !api.isTauri()) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void (async () => {
      try {
        const status = await api.gitStatus(path);
        if (cancelled) return;
        if (!status.available) {
          setError(
            status.reason?.trim() || tr("side.env.commitPushFailed", { error: "git" }),
          );
          return;
        }
        const files = status.files ?? [];
        setFileCount(files.length);
        setNames(
          files
            .slice(0, 6)
            .map((f) => f.name || f.path)
            .filter(Boolean),
        );
        const head = status.branch?.trim() || null;
        setBranch(head || branchHint || null);
        setDetached(!head);
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, projectPath, tr]);

  const busy = phase !== "idle" || loading;
  const needsMessage = !didCommit && commitPushNeedsMessage(fileCount);
  const mode = didCommit ? "push" : commitPushMode(fileCount);
  const canSubmit =
    !!projectPath?.trim() &&
    !busy &&
    !detached &&
    (!needsMessage || message.trim().length > 0);

  const failText = useCallback(
    (reason: string | null | undefined, committedAlready: boolean) => {
      const kind = commitFailureKind(reason);
      if (kind === "detached") return tr("side.env.commitPushDetached");
      if (kind === "message") return tr("side.env.commitPushNeedMessage");
      if (kind === "invalid") return tr("side.env.commitPushInvalidMessage");
      const detail = redactShipOutput(reason || "git", 600);
      return committedAlready
        ? tr("side.env.commitPushCommittedPushFailed", { error: detail })
        : tr("side.env.commitPushFailed", { error: detail });
    },
    [tr],
  );

  const submit = useCallback(async () => {
    const path = projectPath?.trim();
    if (!path || !canSubmit) return;
    setError(null);
    let committedNow = didCommit;
    try {
      if (!committedNow && fileCount > 0) {
        setPhase("committing");
        const commit = await api.gitCommitWorktree(path, message);
        if (!commit.ok) {
          setError(failText(commit.reason, false));
          setPhase("idle");
          return;
        }
        committedNow = !!commit.committed;
        setDidCommit(committedNow);
        if (commit.branch?.trim()) setBranch(commit.branch.trim());
      }
      setPhase("pushing");
      const push = await api.gitPushBranch(path);
      if (!push.ok || !push.available) {
        setError(failText(push.reason, committedNow));
        setPhase("idle");
        return;
      }
      const shown = push.branch?.trim() || branch || "HEAD";
      const done = pushOutputIsUpToDate(push.stdout, push.stderr)
        ? tr("side.env.commitPushUpToDate", { branch: shown })
        : committedNow
          ? tr("side.env.commitPushDone", { branch: shown })
          : tr("side.env.commitPushPushed", { branch: shown });
      onToast?.(done, 4200);
      onDone?.();
      onClose();
    } catch (e) {
      const detail = redactShipOutput(e instanceof Error ? e.message : String(e), 600);
      setError(failText(detail, committedNow));
      setPhase("idle");
    }
  }, [
    branch,
    canSubmit,
    didCommit,
    failText,
    fileCount,
    message,
    onClose,
    onDone,
    onToast,
    projectPath,
    tr,
  ]);

  const actionLabel =
    phase === "committing"
      ? tr("side.env.commitPushCommitting")
      : phase === "pushing"
        ? tr("side.env.commitPushPushing")
        : mode === "commit-push"
          ? tr("side.env.commitPushAction")
          : tr("side.env.commitPushOnly");

  return (
    <GlassModal
      open={open}
      onClose={onClose}
      title={tr("side.env.commitPushTitle")}
      size="md"
      closeLabel={tr("common.close")}
      closeOnOverlay={!busy}
      showClose={!busy}
      wrapBody
      footer={
        <>
          <button
            type="button"
            className="btn btn--ghost"
            disabled={phase !== "idle"}
            onClick={onClose}
          >
            {tr("common.cancel")}
          </button>
          <button
            type="button"
            className="btn btn--solid"
            disabled={!canSubmit}
            aria-busy={phase !== "idle"}
            data-testid="env-commit-push-submit"
            onClick={() => void submit()}
          >
            {actionLabel}
          </button>
        </>
      }
    >
      <form
        className="wt-ship"
        data-testid="env-commit-push"
        onSubmit={(e) => {
          e.preventDefault();
          if (!canSubmit) return;
          void submit();
        }}
      >
        <p className="wt-ship__hint">{tr("side.env.commitPushHint")}</p>
        {branch ? (
          <p className="wt-ship__branch">
            {tr("side.env.commitPushBranch", { branch })}
          </p>
        ) : null}
        {loading ? (
          <p className="wt-ship__status" role="status">
            {tr("side.env.commitPushChecking")}
          </p>
        ) : detached ? (
          <p className="wt-ship__error" role="alert">
            {tr("side.env.commitPushDetached")}
          </p>
        ) : fileCount > 0 && !didCommit ? (
          <div className="wt-ship__status" role="status">
            <div>{tr("side.env.commitPushFiles", { n: fileCount })}</div>
            {names.length > 0 ? <div>{names.join(", ")}</div> : null}
            {fileCount > names.length ? (
              <div>
                {tr("side.env.commitPushMore", {
                  n: fileCount - names.length,
                })}
              </div>
            ) : null}
          </div>
        ) : (
          <p className="wt-ship__status" role="status">
            {tr("side.env.commitPushClean")}
          </p>
        )}
        {needsMessage ? (
          <label className="wt-ship__field">
            <span className="wt-ship__label">
              {tr("side.env.commitPushMessage")}
            </span>
            <textarea
              className="settings-input wt-ship__body"
              value={message}
              onChange={(e) => {
                setMessage(e.target.value);
                setError(null);
              }}
              placeholder={tr("side.env.commitPushMessagePlaceholder")}
              rows={4}
              disabled={phase !== "idle"}
              autoFocus
              spellCheck
              data-testid="env-commit-message"
            />
          </label>
        ) : null}
        {error ? (
          <p className="wt-ship__error" role="alert">
            {error}
          </p>
        ) : null}
      </form>
    </GlassModal>
  );
}
