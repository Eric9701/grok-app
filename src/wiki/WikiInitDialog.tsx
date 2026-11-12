import { useEffect, useState } from "react";
import { GlassModal } from "@/components/GlassModal";
import * as api from "@/lib/api";
import type { MessageKey, Vars } from "@/i18n";
import type { WikiLibrary } from "./libraries";

const TEMPLATES = ["general", "research", "reading", "personal", "business"] as const;

export function WikiInitDialog({
  library,
  onClose,
  onDone,
  t,
}: {
  library: WikiLibrary | null;
  onClose: () => void;
  onDone: () => void;
  t: (key: MessageKey, vars?: Vars) => string;
}) {
  const [template, setTemplate] = useState<(typeof TEMPLATES)[number]>("general");
  const [confirming, setConfirming] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setTemplate("general");
    setConfirming(false);
    setError("");
    setSaving(false);
  }, [library?.id]);

  const submit = async () => {
    if (!library || saving) return;
    setSaving(true);
    setError("");
    try {
      await api.wikiInit(library.path, template);
      onDone();
      onClose();
    } catch {
      setError(t("wiki.initFailed"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <GlassModal
      open={library !== null}
      onClose={onClose}
      title={t("wiki.initTitle")}
      size="md"
      closeLabel={t("common.close")}
      footer={
        confirming ? (
          <>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => setConfirming(false)}
              disabled={saving}
            >
              {t("wiki.initBack")}
            </button>
            <button
              type="button"
              className="btn btn--danger"
              disabled={saving || !library}
              onClick={() => void submit()}
            >
              {t("wiki.initSubmit")}
            </button>
          </>
        ) : (
          <>
            <button type="button" className="btn btn--ghost" onClick={onClose} disabled={saving}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="btn btn--primary"
              disabled={!library}
              onClick={() => setConfirming(true)}
            >
              {t("wiki.initNext")}
            </button>
          </>
        )
      }
    >
      {confirming ? (
        <p className="wiki-error" role="alert">
          {t("wiki.initConfirm", { name: library?.name.trim() || library?.path || "" })}
        </p>
      ) : (
        <>
          <p className="wiki-settings-empty">{t("wiki.initDesc")}</p>
          {library ? (
            <p className="wiki-lib__path" title={library.path}>
              {library.name} · {library.path}
            </p>
          ) : null}
          <div className="wiki-model">
            <span className="wiki-kicker">{t("wiki.template")}</span>
            <div className="wiki-templates" role="radiogroup" aria-label={t("wiki.template")}>
              {TEMPLATES.map((id) => (
                <button
                  key={id}
                  type="button"
                  role="radio"
                  aria-checked={template === id}
                  className={"wiki-template" + (template === id ? " is-selected" : "")}
                  onClick={() => setTemplate(id)}
                >
                  <span className="wiki-template__title">
                    {t(`wiki.template.${id}` as MessageKey)}
                  </span>
                  <span className="wiki-template__desc">
                    {t(`wiki.template.${id}Desc` as MessageKey)}
                  </span>
                </button>
              ))}
            </div>
          </div>
        </>
      )}
      {error ? (
        <p className="wiki-error" role="alert">
          {error}
        </p>
      ) : null}
    </GlassModal>
  );
}
