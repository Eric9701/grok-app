import type { SettingsEntry } from "../types";

export const PET_ENTRIES: readonly SettingsEntry[] = [
  {
    id: "pet.companion",
    section: "pet",
    anchorId: "settings-anchor-pet",
    labelKey: "settings.nav.pet",
    descKeys: [
      "settings.pet.desc",
      "settings.pet.enabled",
      "settings.pet.enabledDesc",
    ],
    keywords: ["pet", "宠物", "寵物", "desktop pet", "companion", "overlay", "mascot"],
  },
  {
    id: "pet.identity",
    section: "pet",
    anchorId: "settings-anchor-pet-identity",
    labelKey: "settings.pet.identity",
    descKeys: ["settings.pet.identityDesc", "settings.pet.shape", "settings.pet.color"],
    keywords: ["shape", "color", "avatar", "hex", "blob", "形状", "颜色"],
  },
  {
    id: "pet.size",
    section: "pet",
    anchorId: "settings-anchor-pet-size",
    labelKey: "settings.pet.size",
    descKeys: ["settings.pet.sizeDesc"],
    keywords: ["size", "scale", "尺寸", "大小"],
  },
];
