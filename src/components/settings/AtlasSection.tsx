/**
 * Settings → Atlas. Relay agent card plus the sidebar cloud-site URL.
 */
import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import { useSettingsModel } from "@/providers/SettingsModelContext";
import { AtlasRelayAgentField } from "./AtlasRelayAgentField";

export const ATLAS_CLOUD_SITE_EVENT = "atlas-cloud-site";

export function publishAtlasCloudSite(url: string) {
  window.dispatchEvent(
    new CustomEvent(ATLAS_CLOUD_SITE_EVENT, { detail: url }),
  );
}

/** Empty string clears the site. http(s) only. */
export function parseAtlasCloudSiteUrl(raw: string): string | null {
  const text = raw.trim();
  if (!text) return "";
  try {
    const url = new URL(text);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    return url.toString();
  } catch {
    return null;
  }
}

function AtlasCloudSiteField({
  t,
  rowHighlight,
}: {
  t: (k: string) => string;
  rowHighlight: (anchorId: string) => string;
}) {
  const [url, setUrl] = useState("");
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      const settings = await api.settingsGet();
      const next = settings.atlasCloudSiteUrl?.trim() || "";
      setUrl(next);
      setError("");
    } catch {
      /* keep the draft */
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const commit = async () => {
    const next = parseAtlasCloudSiteUrl(url);
    if (next == null) {
      setError(t("settings.atlasCloudSiteInvalid"));
      return;
    }
    if (!api.isTauri()) {
      setUrl(next);
      setError("");
      publishAtlasCloudSite(next);
      return;
    }
    try {
      const current = await api.settingsGet();
      const saved = await api.settingsSet({
        ...current,
        atlasCloudSiteUrl: next,
      });
      const stored = saved.atlasCloudSiteUrl?.trim() || next;
      setUrl(stored);
      setError("");
      publishAtlasCloudSite(stored);
    } catch {
      /* keep the draft */
    }
  };

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-atlasCloudSite")}
      id="settings-anchor-atlasCloudSite"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("settings.atlasCloudSite")}</div>
          <div className="settings-row__desc">{t("settings.atlasCloudSiteDesc")}</div>
        </div>
        <input
          className="settings-input"
          value={url}
          placeholder={t("settings.atlasCloudSitePh")}
          onChange={(e) => {
            setUrl(e.target.value);
            setError("");
          }}
          onBlur={() => void commit()}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.currentTarget.blur();
            }
          }}
          aria-label={t("settings.atlasCloudSite")}
          spellCheck={false}
          autoComplete="off"
        />
        {error ? (
          <p className="settings-row__desc" role="alert">
            {error}
          </p>
        ) : null}
      </div>
    </div>
  );
}

export function AtlasSection() {
  const { t, rowHighlight } = useSettingsModel();
  return (
    <>
      <AtlasRelayAgentField t={t} rowHighlight={rowHighlight} />
      <AtlasCloudSiteField t={t} rowHighlight={rowHighlight} />
    </>
  );
}
