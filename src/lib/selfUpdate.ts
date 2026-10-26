/**
 * Atlas desktop self-update (Host `self_update_*`, atlas-server feed).
 *
 * No signing key: the Host checks SHA-256 + size from the server feed. The
 * handle below matches the parts of the Tauri `Update` object that
 * `useUpdater` uses, so the same state machine drives both paths.
 */
import { invoke } from "@tauri-apps/api/core";
import type { MessageKey } from "@/i18n";

export const SELF_UPDATE_PROGRESS_EVENT = "self-update://progress";

export type SelfUpdateFailed = {
  version: string;
  attempts: number;
  notify: boolean;
};

export type SelfUpdateStatus = {
  enabled: boolean;
  currentVersion: string;
  feedUrl: string;
  blocker: string | null;
  lastFailed: SelfUpdateFailed | null;
};

export type SelfUpdateCheck = {
  currentVersion: string;
  updateAvailable: boolean;
  version: string;
  notes: string;
  canAutoInstall: boolean;
  blocker: string | null;
  installerUrl: string | null;
  failedAttempts: number;
  autoPrompt: boolean;
};

export type SelfUpdateProgress = {
  version: string;
  downloaded: number;
  total: number;
};

/** Subset of the Tauri `Update` handle that `useUpdater` relies on. */
export type UpdateHandle = {
  version: string;
  download: () => Promise<unknown>;
  install: () => Promise<unknown>;
  close: () => Promise<unknown>;
};

export const selfUpdateStatus = () => invoke<SelfUpdateStatus>("self_update_status");
export const selfUpdateCheck = () => invoke<SelfUpdateCheck>("self_update_check");
export const selfUpdateAckFailure = () => invoke<void>("self_update_ack_failure");

export function selfUpdateHandle(version: string): UpdateHandle {
  return {
    version,
    download: () => invoke<string>("self_update_download"),
    // Windows: the Host hands off to the installer and exits (never resolves).
    // macOS: resolves after the bundle swap; caller tears down and relaunches.
    install: () => invoke<void>("self_update_install"),
    close: async () => undefined,
  };
}

export function selfUpdatePercent(p: SelfUpdateProgress | null): number | null {
  if (!p || p.total <= 0) return null;
  return Math.min(100, Math.floor((p.downloaded / p.total) * 100));
}

/** `https://atlas.corp/atlas/desktop/stable/update.json` → `atlas.corp`. */
export function feedHost(feedUrl: string): string {
  try {
    return new URL(feedUrl).host;
  } catch {
    return "";
  }
}

const BLOCKER_KEYS: Record<string, MessageKey> = {
  debug_build: "settings.selfUpdate.blocker.debugBuild",
  not_installed_copy: "settings.selfUpdate.blocker.notInstalledCopy",
  translocated: "settings.selfUpdate.blocker.translocated",
  not_writable: "settings.selfUpdate.blocker.notWritable",
  unsupported_platform: "settings.selfUpdate.blocker.unsupportedPlatform",
  no_payload: "settings.selfUpdate.blocker.noPayload",
};

export function selfUpdateBlockerKey(blocker: string | null | undefined): MessageKey | null {
  if (!blocker) return null;
  return BLOCKER_KEYS[blocker] ?? null;
}

/** Host error string → localized explanation (null keeps the generic copy). */
export function selfUpdateErrorKey(message: string | null | undefined): MessageKey | null {
  const m = String(message ?? "").trim();
  if (!m) return null;
  const blocker = selfUpdateBlockerKey(m);
  if (blocker) return blocker;
  if (m === "invalid_mirror") return "settings.selfUpdate.error.invalidMirror";
  if (m === "feed_not_published") return "settings.selfUpdate.error.notPublished";
  if (m === "disk_space") return "settings.selfUpdate.error.diskSpace";
  if (m === "busy") return "settings.selfUpdate.error.busy";
  if (m.startsWith("checksum")) return "settings.selfUpdate.error.checksum";
  if (m.startsWith("network")) return "settings.autoUpdateError.network";
  return null;
}

/** Pop the "Update ready" card only once per version per run. */
export function shouldAutoPromptReady(input: {
  state: string;
  version?: string;
  autoPrompt: boolean;
  dismissedVersion: string | null;
}): boolean {
  return (
    input.state === "ready" &&
    !!input.version &&
    input.autoPrompt &&
    input.dismissedVersion !== input.version
  );
}
