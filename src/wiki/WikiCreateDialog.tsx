import { useState } from "react";
import { GlassModal } from "@/components/GlassModal";
import * as api from "@/lib/api";
import type { MessageKey, Vars } from "@/i18n";
import { createWikiLibrary } from "./createLibrary";
import { displayLibraryPath } from "./libraries";
import type { WikiLibrary } from "./libraries";

const TEMPLATES = ["general", "research", "reading", "personal", "business"] as const;

export function WikiCreateDialog({
  open,
  onClose,
  onCreated,
  t,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (libraries: WikiLibrary[]) => void;
  t: (key: MessageKey, vars?: Vars) => string;
}) {
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [template, setTemplate] = useState<(typeof TEMPLATES)[number]>("general");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const pick = async () => {
    if (!api.isTauri()) return;
    const picked = await api.pickDirectory();
    if (picked) {
      setParent(displayLibraryPath(picked));
      setError("");
    }
  };

  const create = async () => {
    const nextName = name.trim();
    if (!nextName || /[\\/]/.test(nextName) || nextName.includes("..")) {
      setError(t("wiki.errName"));
      return;
    }
    if (!parent.trim()) {
      setError(t("wiki.pathInvalid"));
      return;
    }
    setSaving(true);
    setError("");
    try {
      const libraries = await createWikiLibrary({
        parent,
        name: nextName,
        template,
      });
      onCreated(libraries);
      setName("");
      onClose();
    } catch (err) {
      const text = err instanceof Error ? err.message : String(err);
      setError(text.includes("wiki_duplicate") ? t("wiki.duplicate") : t("wiki.errImport"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <GlassModal
      open={open}
      onClose={onClose}
      title={t("wiki.createTitle")}
      size="md"
      closeLabel={t("common.close")}
      footer={
        <>
          <button type="button" className="btn btn--ghost" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="btn btn--primary"
            disabled={saving}
            onClick={() => void create()}
          >
            {t("wiki.createSubmit")}
          </button>
        </>
      }
    >
      <label className="wiki-model">
        <span className="wiki-kicker">{t("wiki.name")}</span>
        <input
          className="settings-input"
          value={name}
          onChange={(event) => setName(event.target.value)}
          spellCheck={false}
          aria-label={t("wiki.name")}
        />
      </label>
      <label className="wiki-model">
        <span className="wiki-kicker">{t("wiki.createParent")}</span>
        <span className="wiki-create-parent">
          <input
            className="settings-input"
            value={parent}
            readOnly
            aria-label={t("wiki.createParent")}
          />
          <button type="button" className="btn btn--sm btn--ghost" onClick={() => void pick()}>
            {t("wiki.createPick")}
          </button>
        </span>
      </label>
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
              <span className="wiki-template__title">{t(`wiki.template.${id}` as MessageKey)}</span>
              <span className="wiki-template__desc">{t(`wiki.template.${id}Desc` as MessageKey)}</span>
            </button>
          ))}
        </div>
      </div>
      {error ? (
        <p className="wiki-error" role="alert">
          {error}
        </p>
      ) : null}
    </GlassModal>
  );
}
