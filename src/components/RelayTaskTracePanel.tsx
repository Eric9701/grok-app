/**
 * Read-only agent run for one Atlas Relay task.
 * Opening a desktop session is a separate import after the run has finished.
 */
import { useState } from "react";
import type { Locale, MessageKey } from "@/i18n";
import { MarkdownBody } from "@/components/MarkdownBody";
import { useAtlasRelayTaskTrace } from "@/hooks/useAtlasRelayStatus";
import { cliSessionImport, type AtlasRelayTask, type AtlasRelayTraceStep } from "@/lib/api";
import { presentToolCall } from "@/lib/toolPresentation";

type TFn = (key: MessageKey, vars?: Record<string, string | number>) => string;

function traceStatusLabel(tr: TFn, status: string): string {
  if (status === "running") return tr("atlasCloud.running");
  if (status === "done") return tr("atlasCloud.done");
  if (status === "failed") return tr("atlasCloud.failed");
  if (status === "cancelled") return tr("atlasCloud.cancelled");
  return "";
}

function toolStatusLabel(tr: TFn, status: string | null | undefined): string {
  switch ((status || "").trim()) {
    case "pending":
      return tr("kanban.traceTool.pending");
    case "in_progress":
      return tr("kanban.traceTool.in_progress");
    case "completed":
      return tr("kanban.traceTool.completed");
    case "failed":
      return tr("kanban.traceTool.failed");
    case "cancelled":
      return tr("kanban.traceTool.cancelled");
    default:
      return "";
  }
}

function ToolStep({ step, tr }: { step: AtlasRelayTraceStep; tr: TFn }) {
  const meta = presentToolCall({
    toolCallId: step.toolCallId,
    toolKind: step.toolName,
    title: step.title || step.text,
    detail: step.text,
  });
  const label =
    step.title?.trim() ||
    step.text.trim() ||
    step.toolName?.trim() ||
    meta.card;
  const status = toolStatusLabel(tr, step.status);
  const extra = meta.pathBase && meta.pathBase !== label ? meta.pathBase : "";
  return (
    <div className="relay-trace__tool">
      <span className="relay-trace__tool-label" title={label}>
        {label}
      </span>
      {status ? <span className="relay-trace__tool-status">{status}</span> : null}
      {extra ? (
        <span className="relay-trace__tool-extra" title={extra}>
          {extra}
        </span>
      ) : null}
    </div>
  );
}

function TraceStep({
  step,
  locale,
  tr,
}: {
  step: AtlasRelayTraceStep;
  locale: Locale;
  tr: TFn;
}) {
  if (step.kind === "tool") return <ToolStep step={step} tr={tr} />;
  const text = step.text.trim();
  if (!text) return null;
  if (step.kind === "thought") {
    return (
      <details className="relay-trace__thought">
        <summary>{tr("kanban.traceThought")}</summary>
        <MarkdownBody locale={locale}>{step.text}</MarkdownBody>
      </details>
    );
  }
  return (
    <div className="relay-trace__message">
      <MarkdownBody locale={locale}>{step.text}</MarkdownBody>
    </div>
  );
}

export function RelayTaskTracePanel({
  locale,
  tr,
  agentId,
  taskId,
  title,
  tasks,
  onOpenSession,
  onClose,
}: {
  locale: Locale;
  tr: TFn;
  agentId: string;
  taskId: string;
  title: string;
  tasks: readonly AtlasRelayTask[];
  onOpenSession?: (sessionId: string) => void;
  onClose: () => void;
}) {
  const trace = useAtlasRelayTaskTrace(agentId || null, taskId || null);
  const heading = trace?.found ? trace.text.trim() || title : title;
  const status = traceStatusLabel(tr, trace?.status || "");
  const error = trace?.error?.trim() || "";
  const steps = trace?.steps ?? [];
  const self = tasks.find(
    (task) => task.id === taskId && (task.agentId || "") === agentId,
  );
  const sessionId = (trace?.sessionId || self?.sessionId || "").trim();
  const projectId = (trace?.projectId || self?.projectId || "").trim();
  const runStatus = trace?.found ? trace.status : self?.status || "";
  const otherRunning = tasks.some(
    (task) =>
      task.sessionId === sessionId &&
      task.status === "running" &&
      !(task.id === taskId && (task.agentId || "") === agentId),
  );
  const canOpen =
    !!onOpenSession &&
    !!sessionId &&
    (runStatus === "done" || runStatus === "failed") &&
    !otherRunning;
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState("");

  const openDesktopSession = () => {
    if (!canOpen || opening || !onOpenSession) return;
    setOpenError("");
    setOpening(true);
    void cliSessionImport(sessionId, { projectId: projectId || null })
      .then((meta) => {
        const id = meta.id?.trim();
        if (!id) {
          setOpenError(tr("kanban.openDesktopSessionError"));
          return;
        }
        onOpenSession(id);
      })
      .catch(() => {
        setOpenError(tr("kanban.openDesktopSessionError"));
      })
      .finally(() => {
        setOpening(false);
      });
  };

  return (
    <aside className="relay-trace" aria-label={tr("kanban.traceTitle")}>
      <header className="relay-trace__head">
        <div className="relay-trace__heading">
          <p className="relay-trace__kicker">{tr("kanban.traceTitle")}</p>
          <h2 className="relay-trace__title" title={heading}>
            {heading}
          </h2>
          {status ? <p className="relay-trace__status">{status}</p> : null}
        </div>
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          onClick={onClose}
        >
          {tr("kanban.traceClose")}
        </button>
      </header>
      {error ? <p className="relay-trace__error">{error}</p> : null}
      {openError ? <p className="relay-trace__error">{openError}</p> : null}
      <div className="relay-trace__body">
        {steps.length === 0 ? (
          <p className="relay-trace__empty">{tr("kanban.traceEmpty")}</p>
        ) : (
          steps.map((step, index) => (
            <TraceStep
              key={`${step.kind}:${step.toolCallId || index}`}
              step={step}
              locale={locale}
              tr={tr}
            />
          ))
        )}
      </div>
      <footer className="relay-trace__foot">
        {canOpen ? (
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            disabled={opening}
            onClick={openDesktopSession}
          >
            {tr("kanban.openDesktopSession")}
          </button>
        ) : null}
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          onClick={() => {
            window.location.hash = "#/atlas-cloud";
          }}
        >
          {tr("kanban.openRelay")}
        </button>
      </footer>
    </aside>
  );
}
