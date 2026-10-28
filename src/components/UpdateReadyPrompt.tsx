/**
 * Atlas self-update: "Update ready" card once the payload is downloaded and
 * verified. Install only runs after the user picks Update; "Not now" keeps
 * the sidebar dot and About entry for this version until the next launch.
 */
import { useCallback, useEffect, useState } from "react";
import { getName } from "@tauri-apps/api/app";
import {
  UpdateInstallConfirmModal,
  type UpdateInstallConfirmCopy,
} from "@/components/UpdateInstallConfirmModal";
import { useUpdaterContext } from "@/hooks/UpdaterProvider";
import { shouldAutoPromptReady } from "@/lib/selfUpdate";
import * as api from "@/lib/api";
import type { MessageKey } from "@/i18n";

type T = (key: MessageKey, vars?: Record<string, string | number>) => string;

/** Product name from the bundle (white-label safe); empty until resolved. */
function useProductName(): string {
  const [name, setName] = useState("");
  useEffect(() => {
    if (!api.isDesktopHost()) return;
    let alive = true;
    void getName()
      .then((n) => {
        if (alive) setName(n);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);
  return name;
}

/** Copy for the self-update confirm (auto card and sidebar click). */
export function useSelfUpdateConfirmCopy(
  t: T,
  version: string | undefined,
): UpdateInstallConfirmCopy | null {
  const { selfUpdate } = useUpdaterContext();
  const product = useProductName();
  if (!selfUpdate.enabled) return null;
  const v = version?.trim() || "—";
  const failed = selfUpdate.status?.lastFailed;
  const retry = !!failed && failed.version === version;
  return {
    title: t("settings.selfUpdate.prompt.title"),
    message: t(
      retry ? "settings.selfUpdate.prompt.retry" : "settings.selfUpdate.prompt.message",
      { product, version: v },
    ),
    cancel: t("settings.selfUpdate.prompt.later"),
    confirm: t("settings.selfUpdate.update"),
  };
}

export function UpdateReadyPrompt({ t }: { t: T }) {
  const { status, applyAvailableUpdate, selfUpdate } = useUpdaterContext();
  const version = status.state === "ready" ? status.version : undefined;
  const copy = useSelfUpdateConfirmCopy(t, version);
  const open =
    selfUpdate.enabled &&
    shouldAutoPromptReady({
      state: status.state,
      version,
      autoPrompt: selfUpdate.check?.autoPrompt ?? false,
      dismissedVersion: selfUpdate.promptDismissedVersion,
    });

  const later = useCallback(() => {
    if (version) selfUpdate.dismissReadyPrompt(version);
  }, [selfUpdate, version]);

  const update = useCallback(() => {
    if (version) selfUpdate.dismissReadyPrompt(version);
    void applyAvailableUpdate();
  }, [applyAvailableUpdate, selfUpdate, version]);

  if (!open || !copy) return null;
  return (
    <UpdateInstallConfirmModal
      open
      version={version}
      copy={copy}
      t={t}
      onClose={later}
      onConfirm={update}
    />
  );
}
