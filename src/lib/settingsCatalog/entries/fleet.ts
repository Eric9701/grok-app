import type { SettingsEntry } from "../types";

/** Settings → Atlas. Fleet listen card. Kept out of the relay catalog. */
export const FLEET_ENTRIES: readonly SettingsEntry[] = [
  {
    id: "fleet.listen",
    section: "atlas",
    anchorId: "settings-anchor-fleetListen",
    labelKey: "fleet.listen",
    descKeys: [
      "fleet.listenDesc",
      "fleet.bind",
      "fleet.tokenHint",
      "fleet.listening",
      "fleet.savedOnly",
      "fleet.bindError",
      "fleet.copyCommand",
    ],
    keywords: [
      "fleet",
      "worker",
      "websocket",
      "grok-ws-url",
      "机群",
      "监听",
      "派发",
    ],
  },
];
