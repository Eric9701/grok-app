/** Canonical UI message catalog. Keys must stay stable; add all locales together. */

import { en, type MessageKey } from "./en";
import { ru } from "./ru";
import { ruAutomations } from "./ru/automations";
import { ruDoctor } from "./ru/doctor";
import { ruErrors } from "./ru/errors";
import { ruExtensions } from "./ru/extensions";
import { ruExtra } from "./ru/extra";
import { ruProviders } from "./ru/providers";
import { ruSession } from "./ru/session";
import { ruSettingsVisible } from "./ru/settings-visible";
import { ruSlash } from "./ru/slash";
import { ruTasks } from "./ru/tasks";
import { ruWorkspace } from "./ru/workspace";
import { zh } from "./zh";
import { zhTW } from "./zh-TW";

export type Locale = "zh" | "zh-TW" | "ru" | "en";

export type { MessageKey };

export { en };

export const messages: Record<Locale, Record<MessageKey, string>> = {
  en: en as Record<MessageKey, string>,
  ru: {
    ...ru,
    ...ruExtra,
    ...ruErrors,
    ...ruSession,
    ...ruSettingsVisible,
    ...ruSlash,
    ...ruTasks,
    ...ruWorkspace,
    ...ruProviders,
    ...ruAutomations,
    ...ruDoctor,
    ...ruExtensions,
  },
  zh,
  "zh-TW": zhTW,
};

export function isLocale(v: string): v is Locale {
  return v === "zh" || v === "zh-TW" || v === "ru" || v === "en";
}
