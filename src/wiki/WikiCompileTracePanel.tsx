/** Progress for one knowledge-base compile, using the cloud-task trace chrome. */

import { useSyncExternalStore } from "react";
import type { Locale, MessageKey } from "@/i18n";
import { useFeatureMask } from "@/lib/featureMask";
import { formatWikiNotice } from "./errors";
import {
  getWikiCompileTasks,
  subscribeWikiCompileTasks,
  type WikiCompileStage,
  type WikiCompileStep,
} from "./compileTasks";

type TFn = (key: MessageKey, vars?: Record<string, string | number>) => string;

const STAGE_KEY: Record<WikiCompileStage, MessageKey> = {
  read: "kanban.wikiStage.read",
  analyze: "kanban.wikiStage.analyze",
  generate: "kanban.wikiStage.generate",
  write: "kanban.wikiStage.write",
};

const STEP_STATUS_KEY: Record<WikiCompileStep["status"], MessageKey> = {
  pending: "kanban.traceTool.pending",
  in_progress: "kanban.traceTool.in_progress",
  completed: "kanban.traceTool.completed",
  failed: "kanban.traceTool.failed",
  cancelled: "kanban.traceTool.cancelled",
};

function statusLabel(tr: TFn, status: string): string {
  if (status === "running") return tr("atlasCloud.running");
  if (status === "done") return tr("atlasCloud.done");
  if (status === "failed") return tr("atlasCloud.failed");
  return "";
}

export function WikiCompileTracePanel({
  tr,
  taskId,
  onClose,
}: {
  locale: Locale;
  tr: TFn;
  taskId: string;
  onClose: () => void;
}) {
  const wikiVisible = useFeatureMask().wiki;
  const tasks = useSyncExternalStore(
    subscribeWikiCompileTasks,
    getWikiCompileTasks,
    getWikiCompileTasks,
  );
  const task = tasks.find((item) => item.id === taskId);
  const title = task?.sourcePath || tr("kanban.wikiUntitled");
  const status = statusLabel(tr, task?.status || "");
  const error = task?.error ? formatWikiNotice(tr, task.error, task.sourcePath) : "";

  return (
    <aside className="relay-trace" aria-label={tr("kanban.traceTitle")}>
      <header className="relay-trace__head">
        <div className="relay-trace__heading">
          <p className="relay-trace__kicker">{tr("kanban.wikiBadge")}</p>
          <h2 className="relay-trace__title" title={title}>
            {title}
          </h2>
          {status ? <p className="relay-trace__status">{status}</p> : null}
          {task?.projectPath ? (
            <p className="relay-trace__status" title={task.projectPath}>
              {task.projectName || task.projectPath}
            </p>
          ) : null}
        </div>
        <button type="button" className="btn btn--ghost btn--sm" onClick={onClose}>
          {tr("kanban.traceClose")}
        </button>
      </header>
      {error ? <p className="relay-trace__error">{error}</p> : null}
      <div className="relay-trace__body">
        {!task ? (
          <p className="relay-trace__empty">{tr("kanban.traceEmpty")}</p>
        ) : (
          task.steps.map((step) => (
            <div className="relay-trace__tool" key={step.id}>
              <span className="relay-trace__tool-label">{tr(STAGE_KEY[step.id])}</span>
              <span className="relay-trace__tool-status">{tr(STEP_STATUS_KEY[step.status])}</span>
            </div>
          ))
        )}
        {task?.skipped ? <p className="relay-trace__status">{tr("kanban.wikiSkipped")}</p> : null}
        {task && task.written.length > 0 ? (
          <p className="relay-trace__status">
            {tr("kanban.wikiWrote", { count: task.written.length })}
          </p>
        ) : null}
        {task?.totalTokens != null ? (
          <p className="relay-trace__status">
            {tr("wiki.usageTotal", { count: task.totalTokens })}
          </p>
        ) : null}
      </div>
      {wikiVisible ? (
      <footer className="relay-trace__foot">
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          onClick={() => {
            window.location.hash = "#/wiki";
          }}
        >
          {tr("kanban.wikiOpen")}
        </button>
      </footer>
      ) : null}
    </aside>
  );
}
