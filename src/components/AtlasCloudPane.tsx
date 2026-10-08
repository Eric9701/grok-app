/**
 * Read-only list of cloud session/prompt tasks on the Atlas Relay bridge.
 * Desktop chat stays on the local session path.
 */
import { intlLocale, type createT } from "@/i18n";
import type { AtlasRelayAgentStatus } from "@/lib/api";

type TFn = ReturnType<typeof createT>;

function taskTone(status: string): string {
  if (status === "done") return "ok";
  if (status === "failed") return "err";
  if (status === "running") return "warn";
  return "muted";
}

function taskLabel(tr: TFn, status: string): string {
  if (status === "running") return tr("atlasCloud.running");
  if (status === "done") return tr("atlasCloud.done");
  if (status === "failed") return tr("atlasCloud.failed");
  return tr("atlasCloud.cancelled");
}

function formatTaskTime(locale: string, atMs: number): string {
  if (!atMs) return "";
  return new Intl.DateTimeFormat(intlLocale(locale), {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(atMs));
}

export function AtlasCloudPane({
  locale,
  tr,
  status,
}: {
  locale: string;
  tr: TFn;
  status: AtlasRelayAgentStatus | null;
}) {
  const tasks = status?.tasks ?? [];
  const error = status?.error?.trim() || "";
  return (
    <div className="atlas-cloud-page">
      {error ? (
        <p className="atlas-cloud-page__error" role="status">
          {error}
        </p>
      ) : null}
      {tasks.length === 0 ? (
        <p className="atlas-cloud-page__empty">{tr("atlasCloud.empty")}</p>
      ) : (
        <ul className="atlas-cloud-page__list">
          {tasks.map((task) => (
            <li key={`${task.id}-${task.atMs}`} className="atlas-cloud-page__row">
              <span className="atlas-cloud-page__time">
                {formatTaskTime(locale, task.atMs)}
              </span>
              <span className="atlas-cloud-page__text">
                {task.text}
                {task.error ? (
                  <span className="atlas-cloud-page__task-error">{task.error}</span>
                ) : null}
              </span>
              <span
                className={"status-pill status-pill--" + taskTone(task.status)}
                role="status"
              >
                <span className="status-pill__dot" aria-hidden />
                {taskLabel(tr, task.status)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
