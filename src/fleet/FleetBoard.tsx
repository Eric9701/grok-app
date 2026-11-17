import { IconRefresh } from "@/components/icons";
import { Select } from "@/components/Select";
import type { MessageKey } from "@/i18n";
import type { FleetModel, FleetStatus, FleetTask, FleetTrace } from "@/lib/api/fleet";

type TFn = (
  key: MessageKey,
  vars?: Record<string, string | number | null | undefined>,
) => string;

const REASON_KEY: Record<string, MessageKey> = {
  runtime_offline: "fleet.reason.offline",
  environment_prepare_failed: "fleet.reason.prepare",
  runtime_missing_executable: "fleet.cliMissing",
  prompt_failed: "fleet.reason.prompt",
  model_rejected: "fleet.reason.model",
};

function machineBusy(tasks: FleetTask[], agent: string, exceptId: string) {
  return tasks.some(
    (task) =>
      task.id !== exceptId &&
      task.agentId === agent &&
      (task.status === "claimed" || task.status === "running") &&
      !task.unreachable,
  );
}

const STATUS_KEY: Record<string, MessageKey> = {
  queued: "fleet.status.queued",
  claimed: "fleet.status.claimed",
  running: "fleet.status.running",
  completed: "fleet.status.completed",
  failed: "fleet.status.failed",
  cancelled: "fleet.status.cancelled",
};

function statusLabel(tr: TFn, status: string) {
  const key = STATUS_KEY[status];
  return key ? tr(key) : status;
}

function listenTone(status: FleetStatus | null) {
  if (!status) return "idle";
  if (status.listenError) return "bad";
  if (status.listening) return "live";
  return "idle";
}

export function FleetBoard({
  tr,
  status,
  tasks,
  trace,
  agentId,
  cwd,
  prompt,
  models,
  modelId,
  modelsBusy,
  modelsError,
  busy,
  formError,
  onAgentId,
  onCwd,
  onPrompt,
  onModelId,
  onRefreshModels,
  onDispatch,
  onCancel,
  onOpen,
  onCloseTrace,
}: {
  tr: TFn;
  status: FleetStatus | null;
  tasks: FleetTask[];
  trace: FleetTrace | null;
  agentId: string;
  cwd: string;
  prompt: string;
  models: FleetModel[];
  modelId: string;
  modelsBusy: boolean;
  modelsError?: string;
  busy: boolean;
  formError?: string;
  onAgentId: (value: string) => void;
  onCwd: (value: string) => void;
  onPrompt: (value: string) => void;
  onModelId: (value: string) => void;
  onRefreshModels: () => void;
  onDispatch: () => void;
  onCancel: (id: string) => void;
  onOpen: (id: string) => void;
  onCloseTrace: () => void;
}) {
  const remotes = (status?.machines ?? []).filter((machine) => !machine.local);
  const modelOptions = models.length
    ? [
        { value: "", label: tr("fleet.modelDefault") },
        ...models.map((model) => ({
          value: model.id,
          label: modelLabel(model, models),
          keywords: `${model.name} ${model.id}`.toLowerCase(),
        })),
      ]
    : [
        {
          value: "",
          label: modelsBusy ? tr("fleet.modelRefreshing") : tr("fleet.modelEmpty"),
        },
      ];
  const tone = listenTone(status);
  const listenLabel = !status
    ? tr("fleet.listen")
    : status.listening
      ? tr("fleet.listening")
      : status.enabled
        ? tr("fleet.savedOnly")
        : tr("fleet.listen");

  return (
    <div className="fleet-page">
      <div className="fleet-statusline" role="status">
        <span className={`fleet-dot fleet-dot--${tone}`} aria-hidden />
        <span>{listenLabel}</span>
        {status?.savedBind ? (
          <span className="fleet-statusline__bind">{status.savedBind}</span>
        ) : null}
        {status?.tokenLast4 ? (
          <span className="fleet-statusline__token">{status.tokenLast4}</span>
        ) : null}
      </div>
      {status?.listenError ? (
        <p className="fleet-page__error" role="alert">
          {tr("fleet.bindError")}
          <span className="fleet-page__error-detail">{status.listenError}</span>
        </p>
      ) : null}

      {!status ? (
        <div className="fleet-skel" aria-hidden>
          <span />
          <span />
          <span />
        </div>
      ) : (
        <div className="fleet-page__split">
          <div className="fleet-page__side">
            <section className="fleet-block" aria-label={tr("fleet.machines")}>
              <h2 className="fleet-kicker">{tr("fleet.machines")}</h2>
              {remotes.length === 0 ? (
                <div className="fleet-empty">
                  <p>{tr("fleet.machinesEmpty")}</p>
                  <p className="fleet-empty__hint">{tr("fleet.listenDesc")}</p>
                </div>
              ) : null}
              <ul className="fleet-roster">
                {status.machines.map((machine) => (
                  <li key={machine.agentId}>
                    <button
                      type="button"
                      className={
                        "fleet-roster__pick" +
                        (agentId === machine.agentId ? " is-selected" : "")
                      }
                      aria-pressed={agentId === machine.agentId}
                      onClick={() => onAgentId(machine.agentId)}
                    >
                      <span
                        className={
                          "fleet-dot" + (machine.online ? " fleet-dot--live" : "")
                        }
                        aria-hidden
                      />
                      <span className="fleet-roster__name">
                        {machine.local ? tr("fleet.local") : machine.agentId}
                      </span>
                      <span className="fleet-roster__state">
                        {machine.online ? tr("fleet.online") : tr("fleet.offline")}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>

            <form
              className="fleet-form"
              onSubmit={(event) => {
                event.preventDefault();
                if (!busy) onDispatch();
              }}
            >
              <h2 className="fleet-kicker">{tr("fleet.dispatch")}</h2>
              <div className="fleet-form__row">
                <label>
                  <span>{tr("fleet.agent")}</span>
                  <input
                    className="settings-input"
                    value={agentId}
                    onChange={(event) => onAgentId(event.target.value)}
                    spellCheck={false}
                    autoComplete="off"
                  />
                </label>
                <label>
                  <span>{tr("fleet.cwd")}</span>
                  <input
                    className="settings-input"
                    value={cwd}
                    onChange={(event) => onCwd(event.target.value)}
                    spellCheck={false}
                    autoComplete="off"
                    aria-invalid={formError ? true : undefined}
                  />
                  <span className="fleet-form__hint">{tr("fleet.cwdHint")}</span>
                </label>
              </div>
              <div className="fleet-form__field">
                <span>{tr("fleet.model")}</span>
                <div className="fleet-form__model">
                  <Select
                    value={models.some((model) => model.id === modelId) ? modelId : ""}
                    options={modelOptions}
                    onChange={onModelId}
                    disabled={modelsBusy || models.length === 0}
                    aria-label={tr("fleet.model")}
                    searchable={models.length > 8}
                    searchPlaceholder={tr("fleet.model")}
                    emptyLabel={tr("fleet.modelNoMatch")}
                  />
                  <button
                    type="button"
                    className="btn btn--ghost fleet-form__model-refresh"
                    disabled={modelsBusy}
                    aria-label={
                      modelsBusy ? tr("fleet.modelRefreshing") : tr("fleet.modelRefresh")
                    }
                    onClick={() => {
                      if (!modelsBusy) onRefreshModels();
                    }}
                  >
                    <IconRefresh size={14} />
                    {modelsBusy ? tr("fleet.modelRefreshing") : tr("fleet.modelRefresh")}
                  </button>
                </div>
                <span className="fleet-form__hint">{tr("fleet.modelHint")}</span>
                {modelsError ? (
                  <p className="fleet-page__error" role="alert">
                    {modelsError}
                  </p>
                ) : null}
              </div>
              <label>
                <span>{tr("fleet.prompt")}</span>
                <textarea
                  className="settings-input"
                  value={prompt}
                  rows={5}
                  onChange={(event) => onPrompt(event.target.value)}
                />
              </label>
              {formError ? (
                <p className="fleet-page__error" role="alert">
                  {formError}
                </p>
              ) : null}
              <div className="fleet-form__actions">
                <button type="submit" className="btn btn--primary" disabled={busy}>
                  {busy ? tr("fleet.dispatching") : tr("fleet.dispatch")}
                </button>
              </div>
            </form>
          </div>

          <div className={"fleet-page__work" + (trace ? " has-trace" : "")}>
            <section className="fleet-block" aria-label={tr("fleet.tasks")}>
              <h2 className="fleet-kicker">{tr("fleet.tasks")}</h2>
              {tasks.length === 0 ? (
                <div className="fleet-empty">
                  <p>{tr("fleet.tasksEmpty")}</p>
                </div>
              ) : (
                <ul className="fleet-tasks">
                  {tasks.map((task) => {
                    const active =
                      task.status === "queued" ||
                      task.status === "claimed" ||
                      task.status === "running";
                    const selected = trace?.id === task.id;
                    const waiting =
                      task.status === "queued" &&
                      machineBusy(tasks, task.agentId, task.id);
                    const reasonKey = task.failureReason
                      ? REASON_KEY[task.failureReason]
                      : undefined;
                    const retrySource = task.retryOf
                      ? tasks.find((item) => item.id === task.retryOf)
                      : undefined;
                    return (
                      <li key={task.id} className={selected ? "is-selected" : ""}>
                        <button
                          type="button"
                          className="fleet-task"
                          aria-pressed={selected}
                          onClick={() => onOpen(task.id)}
                        >
                          <span className="fleet-task__preview">{task.preview}</span>
                          <span className="fleet-task__meta">
                            <span>
                              {task.agentId === "local" ? tr("fleet.local") : task.agentId}
                            </span>
                            {task.model ? (
                              <span className="fleet-task__model">{task.model}</span>
                            ) : null}
                            <span className={`fleet-pill fleet-pill--${task.status}`}>
                              {statusLabel(tr, task.status)}
                            </span>
                            {task.unreachable ? (
                              <span className="fleet-pill fleet-pill--failed">
                                {tr("fleet.unreachable")}
                              </span>
                            ) : null}
                            {waiting ? <span>{tr("fleet.waiting")}</span> : null}
                            {task.retryOf ? (
                              <span>
                                {tr("fleet.retryOf", {
                                  task:
                                    retrySource?.preview || task.retryOf.slice(0, 8),
                                })}
                              </span>
                            ) : null}
                            {task.status === "failed" && task.failureReason ? (
                              <span>
                                {reasonKey ? tr(reasonKey) : tr("fleet.dispatchFailed")}
                              </span>
                            ) : null}
                          </span>
                        </button>
                        {active ? (
                          <button
                            type="button"
                            className="btn fleet-task__cancel"
                            onClick={() => onCancel(task.id)}
                          >
                            {tr("fleet.cancel")}
                          </button>
                        ) : null}
                      </li>
                    );
                  })}
                </ul>
              )}
            </section>

            {trace ? (
              <aside className="fleet-trace" aria-label={tr("fleet.traceClose")}>
                <div className="fleet-trace__head">
                  <p className="fleet-trace__prompt">{trace.prompt}</p>
                  {trace.model ? <p className="fleet-trace__model">{trace.model}</p> : null}
                  <button type="button" className="btn" onClick={onCloseTrace}>
                    {tr("fleet.traceClose")}
                  </button>
                </div>
                {trace.steps.length === 0 ? (
                  <p className="fleet-empty__hint">{tr("fleet.traceEmpty")}</p>
                ) : (
                  <ol className="fleet-steps">
                    {trace.steps.map((step, index) => (
                      <li key={`${step.kind}-${index}`} data-kind={step.kind}>
                        {step.title || step.toolName ? (
                          <span className="fleet-steps__name">
                            {step.title || step.toolName}
                          </span>
                        ) : null}
                        {step.text ? <p>{step.text}</p> : null}
                      </li>
                    ))}
                  </ol>
                )}
              </aside>
            ) : null}
          </div>
        </div>
      )}
    </div>
  );
}

function modelLabel(model: FleetModel, models: FleetModel[]) {
  const name = model.name.trim() || model.id;
  const same = models.filter((item) => (item.name.trim() || item.id) === name).length;
  return same > 1 && name !== model.id ? `${name} · ${model.id}` : name;
}
