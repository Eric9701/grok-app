/**
 * Settings → Runtime → CLI: enterprise Atlas CLI install/update base.
 *
 * Persists `atlasCliMirror` on AppSettings. Host also honors `ATLAS_CLI_MIRROR`.
 */
import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { Vars } from "@/i18n";

type Props = {
  t: (k: string, vars?: Vars) => string;
  rowHighlight: (anchorId: string) => string;
};

export function AtlasCliMirrorField({ t, rowHighlight }: Props) {
  const [value, setValue] = useState("");
  const [saving, setSaving] = useState(false);

  const refresh = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      const s = await api.settingsGet();
      setValue(s.atlasCliMirror?.trim() || "");
    } catch {
      /* soft-fail: keep local draft */
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const persist = async (next: string) => {
    if (!api.isTauri() || saving) return;
    setSaving(true);
    try {
      const s = await api.settingsGet();
      const trimmed = next.trim();
      await api.settingsSet({
        ...s,
        atlasCliMirror: trimmed || null,
      });
      setValue(trimmed);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-atlasCliMirror")}
      id="settings-anchor-atlasCliMirror"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("settings.atlasCliMirror")}</div>
          <div className="settings-row__desc">{t("settings.atlasCliMirrorDesc")}</div>
        </div>
        <input
          className="settings-input"
          value={value}
          placeholder={t("settings.atlasCliMirrorPlaceholder")}
          disabled={saving}
          onChange={(e) => setValue(e.target.value)}
          onBlur={(e) => {
            void persist(e.target.value);
          }}
          aria-label={t("settings.atlasCliMirror")}
        />
      </div>
    </div>
  );
}
