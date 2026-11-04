/**
 * Settings → Runtime → CLI: read-only `[endpoints]` from `~/.atlas/config.toml`.
 *
 * Shared CLI home is not rewritten. The host returns only this table.
 */
import { Fragment, useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { AtlasEndpointsSnapshot } from "@/lib/api";
import type { MessageKey, Vars } from "@/i18n";

type Props = {
  t: (k: MessageKey, vars?: Vars) => string;
  rowHighlight: (anchorId: string) => string;
};

const KNOWN_LABELS: Record<string, MessageKey> = {
  cli_chat_proxy_base_url: "settings.atlasEndpoints.chatProxy",
  cli_update_base_url: "settings.atlasEndpoints.updateBase",
};

export function AtlasEndpointsCard({ t, rowHighlight }: Props) {
  const [snap, setSnap] = useState<AtlasEndpointsSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [desktop, setDesktop] = useState(true);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    if (!api.isTauri()) {
      setDesktop(false);
      setSnap(null);
      setError(null);
      return;
    }
    setDesktop(true);
    setError(null);
    try {
      setSnap(await api.atlasEndpointsRead());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void load().finally(() => {
      if (live) setLoading(false);
    });
    return () => {
      live = false;
    };
  }, [load]);

  const refresh = () => {
    if (refreshing || loading) return;
    setRefreshing(true);
    void load().finally(() => setRefreshing(false));
  };

  const entries = snap?.entries ?? [];
  const readError = error || snap?.error || null;

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-atlasEndpoints")}
      id="settings-anchor-atlasEndpoints"
      data-testid="settings-atlas-endpoints"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("settings.atlasEndpoints")}</div>
          <div className="settings-row__desc">{t("settings.atlasEndpointsDesc")}</div>
        </div>

        {loading ? (
          <div className="settings-row__hint">{t("settings.atlasEndpoints.loading")}</div>
        ) : null}

        {!loading && !desktop ? (
          <div className="settings-row__hint">{t("settings.atlasEndpoints.needDesktop")}</div>
        ) : null}

        {readError ? (
          <div className="settings-row__hint settings-row__hint--warn">
            {t("settings.atlasEndpoints.error", { error: readError })}
          </div>
        ) : null}

        {!loading && desktop && snap && !snap.exists && !readError ? (
          <div className="settings-row__hint">{t("settings.atlasEndpoints.missingFile")}</div>
        ) : null}

        {!loading && snap?.exists && entries.length === 0 && !readError ? (
          <div className="settings-row__hint">{t("settings.atlasEndpoints.missingSection")}</div>
        ) : null}

        {snap?.truncated ? (
          <div className="settings-row__hint">{t("settings.atlasEndpoints.truncated")}</div>
        ) : null}

        {entries.map((entry, index) => {
          const labelKey = KNOWN_LABELS[entry.key];
          const label = labelKey ? t(labelKey) : entry.key;
          return (
            <Fragment key={`${entry.key}-${index}`}>
              <div className="settings-row__text">
                <div className="settings-row__label">{label}</div>
                {labelKey ? <div className="settings-row__desc">{entry.key}</div> : null}
              </div>
              <input
                className="settings-input"
                readOnly
                spellCheck={false}
                value={entry.value}
                placeholder={t("settings.atlasEndpoints.empty")}
                aria-label={labelKey ? `${label} (${entry.key})` : entry.key}
                data-endpoint-key={entry.key}
              />
            </Fragment>
          );
        })}

        {snap?.path ? (
          <div className="settings-row__hint">
            {t("settings.atlasEndpoints.source", { path: snap.path })}
          </div>
        ) : null}

        {desktop ? (
          <div className="settings-row__actions">
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              disabled={loading || refreshing}
              onClick={refresh}
            >
              {refreshing
                ? t("settings.atlasEndpoints.loading")
                : t("settings.atlasEndpoints.refresh")}
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
