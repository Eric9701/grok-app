/**
 * Workbench location.hash routing (settings overlay vs in-app panes).
 * Pure — no DOM writes. Last-route restore stays in the navigation hook.
 */
import {
  isSettingsSectionId,
  parseSettingsHash,
  type SettingsSectionId,
  type SettingsTabId,
} from "@/lib/settingsCatalog";
import { parsePrHubDeepLink } from "@/lib/prHubDeepLink";

export type WorkbenchHashPane = "chat" | "automations" | "kanban" | "octo" | "atlasCloud";

export type WorkbenchHashRoute =
  | {
      kind: "settings-explicit";
      section: SettingsSectionId;
      tab: SettingsTabId | null;
      prNumber: number | null;
    }
  | { kind: "settings-last" }
  | { kind: "pane"; pane: WorkbenchHashPane };

/**
 * Parse `#/settings…` / `#/automations` / `#/kanban` / `#/octo` / `#/atlas-cloud` / empty workbench hashes.
 */
export function resolveWorkbenchHash(
  fullHash: string | null | undefined,
): WorkbenchHashRoute {
  const raw = (fullHash || "").replace(/^#\/?/, "");
  if (raw.startsWith("settings")) {
    const parts = raw.split("/").filter(Boolean);
    const sectionPart = (parts[1] ?? "").split("?")[0];
    if (isSettingsSectionId(sectionPart)) {
      const loc = parseSettingsHash(raw);
      const prHub = parsePrHubDeepLink(fullHash ?? "");
      return {
        kind: "settings-explicit",
        section: loc?.section ?? sectionPart,
        tab: loc?.tab ?? null,
        prNumber: prHub?.prNumber ?? null,
      };
    }
    return { kind: "settings-last" };
  }
  if (raw === "automations" || raw.startsWith("automations")) {
    return { kind: "pane", pane: "automations" };
  }
  if (raw === "kanban" || raw.startsWith("kanban")) {
    return { kind: "pane", pane: "kanban" };
  }
  if (raw === "octo" || raw.startsWith("octo")) {
    return { kind: "pane", pane: "octo" };
  }
  if (raw === "atlas-cloud" || raw.startsWith("atlas-cloud")) {
    return { kind: "pane", pane: "atlasCloud" };
  }
  return { kind: "pane", pane: "chat" };
}
