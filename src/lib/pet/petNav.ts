/**
 * Shared navigation contract: settings 宠物 menu and pet-window 编辑
 * must open the same destination.
 */

import { buildSettingsHash } from "@/lib/settingsCatalog";

export const PET_SETTINGS_SECTION = "pet" as const;

/** Canonical hash for Settings → 宠物. */
export const PET_SETTINGS_HASH = buildSettingsHash({
  section: PET_SETTINGS_SECTION,
});

export function petSettingsHash(): string {
  return PET_SETTINGS_HASH;
}
