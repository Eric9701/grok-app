/**
 * Settings → CLI / runtime → Diagnostics: check and repair config.toml.
 * State stays in this card (not App / AppWorkbench).
 */
import { useCallback, useEffect, useState } from "react";
import { GlassModal } from "@/components/GlassModal";
import { IconDoctor } from "@/components/icons";
import type { SettingsViewModel } from "@/components/settings/types";
import type { Vars } from "@/i18n";
import * as api from "@/lib/api";
import type { ConfigTomlDiagnosis } from "@/lib/api";

type Props = {
  highlight?: string;
  mode?: string;
  showSettingsToast?: SettingsViewModel["showSettingsToast"];
  t: (k: string, vars?: Vars) => string;
};

function statusText(t: Props["t"], diagnosis: ConfigTomlDiagnosis): string {
  const examples = diagnosis.examples.length
    ? diagnosis.examples.join(", ")
    : "—";
  switch (diagnosis.status) {
    case "ok":
      return t("settings.configTomlStatusOk");
    case "missing":
      return t("settings.configTomlStatusMissing");
    case "empty":
      return t("settings.configTomlStatusEmpty");
    case "duplicate":
      return t("settings.configTomlStatusDuplicate", {
        n: diagnosis.duplicateCount,
        examples,
      });
    case "syntax":
      return t("settings.configTomlStatusSyntax");
    default:
      return t("settings.configTomlStatusUnreadable");
  }
}

export function ConfigTomlRepairCard({
  highlight = "",
  mode,
  showSettingsToast,
  t,
}: Props) {
  const [diagnosis, setDiagnosis] = useState<ConfigTomlDiagnosis | null>(null);
  const [phase, setPhase] = useState<"idle" | "checking" | "repairing">(
    "checking",
  );
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [refreshToken, setRefreshToken] = useState(0);
  const desktop = api.isTauri();

  useEffect(() => {
    if (!desktop) {
      setPhase("idle");
      setDiagnosis(null);
      return;
    }
    let cancelled = false;
    setPhase("checking");
    setError(null);
    void (async () => {
      try {
        const next = await api.configTomlDiagnose(mode);
        if (!cancelled) setDiagnosis(next);
      } catch (e) {
        if (!cancelled) {
          setDiagnosis(null);
          const message = e instanceof Error ? e.message : String(e);
          setError(t("settings.configTomlRepairFailed", { error: message }));
        }
      } finally {
        if (!cancelled) setPhase("idle");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [desktop, mode, refreshToken, t]);

  const repair = useCallback(async () => {
    if (phase !== "idle" || !diagnosis?.repairable) return;
    setConfirmOpen(false);
    setPhase("repairing");
    setError(null);
    try {
      const result = await api.configTomlRepair(mode);
      const backup = result.backupPath || "—";
      const message =
        result.action === "deduped"
          ? t("settings.configTomlRepairedDeduped", {
              n: result.removedDuplicates,
              backup,
            })
          : result.action === "reset"
            ? t("settings.configTomlRepairedReset", { backup })
            : t("settings.configTomlRepairNoop");
      setNotice(message);
      showSettingsToast?.(message, 4200);
      setRefreshToken((n) => n + 1);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(t("settings.configTomlRepairFailed", { error: message }));
      setPhase("idle");
    }
  }, [diagnosis?.repairable, mode, phase, showSettingsToast, t]);

  const busy = phase !== "idle";
  const modeLabel =
    diagnosis?.mode === "shared"
      ? t("settings.configTomlModeShared")
      : diagnosis?.mode === "independent"
        ? t("settings.configTomlModeIndependent")
        : "";
  const confirmReset = diagnosis?.status !== "duplicate";

  return (
    <div
      className={"settings-card" + highlight}
      id="settings-anchor-config-toml"
      data-testid="settings-config-toml-repair"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">
            <IconDoctor size={16} />
            {t("settings.configTomlRepair")}
          </div>
          <div className="settings-row__desc">
            {t("settings.configTomlRepairDesc")}
          </div>
          {!desktop ? (
            <div className="settings-row__hint">
              {t("settings.configTomlNeedDesktop")}
            </div>
          ) : null}
          {diagnosis?.path ? (
            <div className="settings-row__hint" title={diagnosis.path}>
              {modeLabel ? `${modeLabel} · ` : ""}
              {t("settings.configTomlEdit.path", { path: diagnosis.path })}
            </div>
          ) : null}
          {desktop && phase === "checking" && !diagnosis ? (
            <div className="settings-row__hint" role="status">
              {t("settings.configTomlRepairChecking")}
            </div>
          ) : null}
          {diagnosis ? (
            <div className="settings-row__hint" role="status">
              {statusText(t, diagnosis)}
            </div>
          ) : null}
          {notice ? (
            <div className="settings-row__hint" role="status">
              {notice}
            </div>
          ) : null}
          {error ? (
            <div className="settings-row__hint settings-row__hint--warn" role="alert">
              {error}
            </div>
          ) : null}
        </div>
        <div className="settings-row__actions">
          <button
            type="button"
            className="btn btn--ghost settings-row__action"
            disabled={!desktop || busy || !diagnosis?.repairable}
            aria-busy={busy}
            onClick={() => setConfirmOpen(true)}
          >
            {phase === "checking"
              ? t("settings.configTomlRepairChecking")
              : phase === "repairing"
                ? t("settings.configTomlRepairRunning")
                : t("settings.configTomlRepairAction")}
          </button>
        </div>
      </div>
      <GlassModal
        open={confirmOpen}
        onClose={() => setConfirmOpen(false)}
        title={t("settings.configTomlConfirmTitle")}
        size="sm"
        closeLabel={t("common.close")}
        footer={
          <>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => setConfirmOpen(false)}
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className={confirmReset ? "btn btn--danger" : "btn btn--solid"}
              onClick={() => void repair()}
            >
              {t("settings.configTomlConfirmAction")}
            </button>
          </>
        }
      >
        <p className="settings-row__desc is-flush">
          {confirmReset
            ? t("settings.configTomlConfirmReset")
            : t("settings.configTomlConfirmDeduped")}
        </p>
      </GlassModal>
    </div>
  );
}
