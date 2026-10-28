/**
 * In-app confirm before signed install+relaunch.
 * No window.confirm — GlassModal only.
 */
import { GlassModal } from "@/components/GlassModal";
import { updateInstallConfirmCopyKeys } from "@/lib/appUpdateHonesty";
import type { MessageKey } from "@/i18n";

/** Already-localized strings that replace the default confirm copy. */
export type UpdateInstallConfirmCopy = {
  title: string;
  message: string;
  cancel: string;
  confirm: string;
};

export function UpdateInstallConfirmModal({
  open,
  version,
  busy = false,
  copy,
  t,
  onClose,
  onConfirm,
}: {
  open: boolean;
  version?: string | null;
  busy?: boolean;
  copy?: UpdateInstallConfirmCopy | null;
  t: (key: MessageKey, vars?: Record<string, string | number>) => string;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const keys = updateInstallConfirmCopyKeys();
  const versionLabel =
    typeof version === "string" && version.trim() ? version.trim() : "—";
  const close = () => {
    if (busy) return;
    onClose();
  };

  return (
    <GlassModal
      open={open}
      onClose={close}
      title={copy?.title ?? t(keys.titleKey)}
      size="sm"
      closeLabel={t("common.close")}
      closeOnOverlay={!busy}
      showClose={!busy}
      wrapBody
      className="update-install-confirm-modal"
      footer={
        <>
          <button
            type="button"
            className="btn btn--ghost"
            disabled={busy}
            onClick={onClose}
          >
            {copy?.cancel ?? t(keys.cancelKey)}
          </button>
          <button
            type="button"
            className="btn btn--solid"
            disabled={busy}
            onClick={onConfirm}
          >
            {copy?.confirm ?? t(keys.confirmKey)}
          </button>
        </>
      }
    >
      <p className="settings-row__desc" style={{ margin: 0 }}>
        {copy?.message ?? t(keys.messageKey, { version: versionLabel })}
      </p>
    </GlassModal>
  );
}
