/**
 * Sidebar Atlas Cloud: embedded page from Settings → Atlas.
 * Stays mounted after the first open so switching menus does not reload it.
 * The address stays off the chrome, same as Octo.
 */
import { useEffect, useRef, useState } from "react";
import { EmbeddedBrowser } from "@/components/EmbeddedBrowser";
import { ATLAS_CLOUD_SITE_EVENT } from "@/components/settings/AtlasSection";
import type { Locale, createT } from "@/i18n";
import * as api from "@/lib/api";

type TFn = ReturnType<typeof createT>;

function readSiteDetail(event: Event): string {
  const detail = (event as CustomEvent<string>).detail;
  return typeof detail === "string" ? detail.trim() : "";
}

export function AtlasCloudPane({
  locale,
  title,
  tr,
  active,
  siteUrl,
}: {
  locale: Locale;
  title: string;
  tr: TFn;
  active: boolean;
  /** Test override. Undefined loads the saved setting. */
  siteUrl?: string;
}) {
  const [url, setUrl] = useState(siteUrl?.trim() ?? "");

  useEffect(() => {
    if (siteUrl != null) {
      setUrl(siteUrl.trim());
      return;
    }
    let stop = false;
    const load = () => {
      if (!api.isTauri()) return;
      void api.settingsGet().then((settings) => {
        if (!stop) setUrl(settings.atlasCloudSiteUrl?.trim() || "");
      }).catch(() => undefined);
    };
    const onSite = (event: Event) => {
      if (!stop) setUrl(readSiteDetail(event));
    };
    load();
    window.addEventListener(ATLAS_CLOUD_SITE_EVENT, onSite);
    return () => {
      stop = true;
      window.removeEventListener(ATLAS_CLOUD_SITE_EVENT, onSite);
    };
  }, [siteUrl]);

  return (
    <div className="atlas-cloud-pane" hidden={!active}>
      {url ? (
        <EmbeddedBrowser
          url={url}
          title={title}
          locale={locale}
          instanceId="atlas-cloud"
          active={active}
          showAddress={false}
        />
      ) : (
        <div className="atlas-cloud-page">
          <p className="atlas-cloud-page__empty">{tr("atlasCloud.siteEmpty")}</p>
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => {
              window.location.hash = "#/settings/atlas";
            }}
          >
            {tr("atlasCloud.openSettings")}
          </button>
        </div>
      )}
    </div>
  );
}

/** Mount on first open and keep the webview alive across pane switches. */
export function AtlasCloudSlot({
  open,
  locale,
  title,
  tr,
}: {
  open: boolean;
  locale: Locale;
  title: string;
  tr: TFn;
}) {
  const keptRef = useRef(open);
  if (open) keptRef.current = true;
  if (!keptRef.current) return null;
  return (
    <AtlasCloudPane locale={locale} title={title} tr={tr} active={open} />
  );
}
