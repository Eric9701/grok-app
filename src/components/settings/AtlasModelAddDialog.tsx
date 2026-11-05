import { useState } from "react";
import { GlassModal } from "@/components/GlassModal";
import { Select } from "@/components/Select";
import * as api from "@/lib/api";
import { catalogFromAvailableModels } from "@/lib/availableModelsCatalog";
import type { ModelOption } from "@/lib/grokCatalog";
import type { MessageKey } from "@/i18n";

type TFn = (key: MessageKey, vars?: Record<string, string | number>) => string;

const BACKENDS = ["messages", "chat_completions", "responses"] as const;

const ERROR_KEYS: Record<string, MessageKey> = {
  duplicate_id: "settings.availableModelsAddErr.duplicate_id",
  invalid_id: "settings.availableModelsAddErr.invalid_id",
  reserved_id: "settings.availableModelsAddErr.reserved_id",
  missing_field: "settings.availableModelsAddErr.missing_field",
  invalid_backend: "settings.availableModelsAddErr.invalid_backend",
  invalid_context_window: "settings.availableModelsAddErr.invalid_context_window",
  invalid_base_url: "settings.availableModelsAddErr.invalid_base_url",
  default_changed: "settings.availableModelsAddErr.default_changed",
};

type FieldId = "id" | "model" | "name" | "baseUrl" | "context" | "apiKey";

function errorText(t: TFn, raw: string): string {
  const code = raw.trim();
  const key = ERROR_KEYS[code];
  if (key) return t(key);
  const failed = t("settings.availableModelsAddFailed");
  return code && code !== failed ? `${failed} ${code}` : failed;
}

function hostInvalid(raw: string): FieldId | null {
  const code = raw.trim();
  if (code === "invalid_id" || code === "reserved_id" || code === "duplicate_id") return "id";
  if (code === "invalid_base_url") return "baseUrl";
  if (code === "invalid_context_window") return "context";
  return null;
}

export function AtlasModelAddDialog({
  open,
  onClose,
  t,
  showToast,
  onAvailableModels,
}: {
  open: boolean;
  onClose: () => void;
  t: TFn;
  showToast: (message: string) => void;
  onAvailableModels?: (models: ModelOption[]) => void;
}) {
  const [id, setId] = useState("");
  const [model, setModel] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiBackend, setApiBackend] = useState<string>("messages");
  const [contextWindow, setContextWindow] = useState("200000");
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [invalid, setInvalid] = useState<ReadonlySet<FieldId>>(new Set());

  const clearInvalid = (field: FieldId) => {
    setInvalid((prev) => {
      if (!prev.has(field)) return prev;
      const next = new Set(prev);
      next.delete(field);
      return next;
    });
  };

  const close = () => {
    if (busy) return;
    setError(null);
    onClose();
  };

  const submit = () => {
    if (busy) return;
    const windowTokens = Number(contextWindow.trim());
    const missing = new Set<FieldId>();
    if (!id.trim()) missing.add("id");
    if (!model.trim()) missing.add("model");
    if (!name.trim()) missing.add("name");
    if (!baseUrl.trim()) missing.add("baseUrl");
    if (!apiKey.trim()) missing.add("apiKey");
    const contextBad = !Number.isInteger(windowTokens) || windowTokens <= 0;
    if (contextBad) missing.add("context");
    const urlBad =
      baseUrl.trim().length > 0 &&
      !/^https?:\/\//i.test(baseUrl.trim());
    if (urlBad) missing.add("baseUrl");
    if (missing.size > 0) {
      setInvalid(missing);
      if (
        !id.trim() ||
        !model.trim() ||
        !name.trim() ||
        !baseUrl.trim() ||
        !apiKey.trim()
      ) {
        setError(t("settings.availableModelsAddErr.missing_field"));
      } else if (contextBad) {
        setError(t("settings.availableModelsAddErr.invalid_context_window"));
      } else {
        setError(t("settings.availableModelsAddErr.invalid_base_url"));
      }
      return;
    }
    setBusy(true);
    setError(null);
    setInvalid(new Set());
    void api
      .providersAtlasModelAdd({
        id: id.trim(),
        model: model.trim(),
        name: name.trim(),
        description: description.trim(),
        baseUrl: baseUrl.trim(),
        apiBackend,
        contextWindow: windowTokens,
        apiKey: apiKey.trim(),
      })
      .then(async () => {
        try {
          const res = await api.modelsListAvailable();
          const next = catalogFromAvailableModels(res);
          if (next.length > 0) onAvailableModels?.(next);
        } catch {
          /* The section is on disk; the next refresh still reads it. */
        }
        setId("");
        setModel("");
        setName("");
        setDescription("");
        setBaseUrl("");
        setApiBackend("messages");
        setContextWindow("200000");
        setApiKey("");
        setShowKey(false);
        showToast(t("settings.availableModelsAddDone"));
        onClose();
      })
      .catch((err: unknown) => {
        const detail = err instanceof Error ? err.message : String(err ?? "");
        const field = hostInvalid(detail);
        setInvalid(field ? new Set([field]) : new Set());
        setError(errorText(t, detail));
      })
      .finally(() => setBusy(false));
  };

  return (
    <GlassModal
      open={open}
      onClose={close}
      title={t("settings.availableModelsAddTitle")}
      size="lg"
      className="settings-atlas-model-modal"
      wrapBody
      closeLabel={t("common.close")}
      closeOnOverlay={!busy}
      footer={
        <>
          <button
            type="button"
            className="btn btn--ghost"
            disabled={busy}
            onClick={close}
          >
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="btn btn--solid"
            disabled={busy}
            onClick={submit}
          >
            {busy
              ? t("settings.availableModelsAdding")
              : t("settings.availableModelsAddSubmit")}
          </button>
        </>
      }
    >
      <p className="settings-atlas-model-form__hint">
        {t("settings.availableModelsAddHint")}
      </p>
      <div className="settings-atlas-model-form">
        <section className="settings-atlas-model-group">
          <h3 className="settings-atlas-model-group__title">
            {t("settings.availableModelsAddGroupIdentity")}
          </h3>
          <div className="settings-atlas-model-grid">
            <Field
              label={t("settings.availableModelsAddId")}
              value={id}
              disabled={busy}
              mono
              invalid={invalid.has("id")}
              placeholder={t("settings.availableModelsAddPhId")}
              autoComplete="off"
              onChange={(value) => {
                clearInvalid("id");
                setId(value);
              }}
            />
            <Field
              label={t("settings.availableModelsAddName")}
              value={name}
              disabled={busy}
              invalid={invalid.has("name")}
              placeholder={t("settings.availableModelsAddPhName")}
              autoComplete="off"
              onChange={(value) => {
                clearInvalid("name");
                setName(value);
              }}
            />
            <Field
              wide
              label={t("settings.availableModelsAddModel")}
              value={model}
              disabled={busy}
              mono
              invalid={invalid.has("model")}
              placeholder={t("settings.availableModelsAddPhModel")}
              autoComplete="off"
              onChange={(value) => {
                clearInvalid("model");
                setModel(value);
              }}
            />
            <Field
              wide
              label={t("settings.availableModelsAddDescription")}
              optional={t("settings.availableModelsAddOptional")}
              value={description}
              disabled={busy}
              placeholder={t("settings.availableModelsAddPhDescription")}
              autoComplete="off"
              onChange={setDescription}
            />
          </div>
        </section>
        <section className="settings-atlas-model-group">
          <h3 className="settings-atlas-model-group__title">
            {t("settings.availableModelsAddGroupConnection")}
          </h3>
          <div className="settings-atlas-model-grid">
            <Field
              wide
              label={t("settings.availableModelsAddBaseUrl")}
              value={baseUrl}
              disabled={busy}
              mono
              invalid={invalid.has("baseUrl")}
              placeholder={t("settings.availableModelsAddPhBaseUrl")}
              autoComplete="off"
              spellCheck={false}
              onChange={(value) => {
                clearInvalid("baseUrl");
                setBaseUrl(value);
              }}
            />
            <label className="settings-atlas-model-field">
              <span className="settings-atlas-model-field__label">
                {t("settings.availableModelsAddBackend")}
              </span>
              <Select
                value={apiBackend}
                disabled={busy}
                aria-label={t("settings.availableModelsAddBackend")}
                onChange={setApiBackend}
                options={BACKENDS.map((value) => ({
                  value,
                  label: t(
                    value === "messages"
                      ? "prov.protocol.messages"
                      : value === "chat_completions"
                        ? "prov.protocol.chatCompletions"
                        : "prov.protocol.responses",
                  ),
                }))}
              />
            </label>
            <Field
              label={t("settings.availableModelsAddContext")}
              value={contextWindow}
              disabled={busy}
              mono
              invalid={invalid.has("context")}
              placeholder={t("settings.availableModelsAddPhContext")}
              inputMode="numeric"
              autoComplete="off"
              onChange={(value) => {
                clearInvalid("context");
                setContextWindow(value);
              }}
            />
            <div className="settings-atlas-model-field settings-atlas-model-field--wide">
              <span className="settings-atlas-model-field__label">
                {t("settings.availableModelsAddApiKey")}
              </span>
              <div className="prov-key-row">
                <input
                  className={
                    "settings-input settings-input--mono" +
                    (invalid.has("apiKey") ? " is-invalid" : "")
                  }
                  type={showKey ? "text" : "password"}
                  value={apiKey}
                  disabled={busy}
                  placeholder={t("settings.availableModelsAddPhApiKey")}
                  autoComplete="off"
                  spellCheck={false}
                  aria-invalid={invalid.has("apiKey") || undefined}
                  onChange={(e) => {
                    clearInvalid("apiKey");
                    setApiKey(e.target.value);
                  }}
                />
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  disabled={busy}
                  onClick={() => setShowKey((v) => !v)}
                >
                  {showKey ? t("prov.keyHide") : t("prov.keyShow")}
                </button>
              </div>
            </div>
          </div>
        </section>
      </div>
      {error ? (
        <p className="settings-atlas-model-alert" role="alert">
          {error}
        </p>
      ) : null}
    </GlassModal>
  );
}

function Field({
  label,
  optional,
  value,
  onChange,
  disabled,
  placeholder,
  type = "text",
  autoComplete,
  spellCheck,
  inputMode,
  mono,
  wide,
  invalid,
}: {
  label: string;
  optional?: string;
  value: string;
  onChange: (value: string) => void;
  disabled: boolean;
  placeholder?: string;
  type?: string;
  autoComplete?: string;
  spellCheck?: boolean;
  inputMode?: "numeric" | "text";
  mono?: boolean;
  wide?: boolean;
  invalid?: boolean;
}) {
  return (
    <label
      className={
        "settings-atlas-model-field" +
        (wide ? " settings-atlas-model-field--wide" : "")
      }
    >
      <span className="settings-atlas-model-field__label">
        <span>{label}</span>
        {optional ? (
          <span className="settings-atlas-model-field__optional">{optional}</span>
        ) : null}
      </span>
      <input
        className={
          "settings-input" +
          (mono ? " settings-input--mono" : "") +
          (invalid ? " is-invalid" : "")
        }
        type={type}
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        autoComplete={autoComplete}
        spellCheck={spellCheck}
        inputMode={inputMode}
        aria-invalid={invalid || undefined}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}
