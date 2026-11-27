/**
 * Main-pane Tabularis. The child webview stays mounted after the first open
 * so SQL tabs survive a switch back to chat.
 */
import { useEffect, useRef, useState } from "react";
import { EmbeddedBrowser } from "@/components/EmbeddedBrowser";
import type { Locale } from "@/i18n";
import { createT } from "@/i18n";
import {
  deliverSqlToDatabase,
  syncTabularisHost,
  tabularisEnsureStarted,
  type TabularisStatus,
} from "@/lib/api/tabularis";
import { isTauri } from "@/lib/api/host";
import { clearQueuedDatabaseSql, useQueuedDatabaseSql } from "@/lib/tabularisStore";

export function DatabasePane({
  locale,
  title,
  active,
}: {
  locale: Locale;
  title: string;
  active: boolean;
}) {
  const tr = createT(locale);
  const [status, setStatus] = useState<TabularisStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pageReady, setPageReady] = useState(false);
  const queuedSql = useQueuedDatabaseSql();
  const url = status?.bootstrapUrl?.trim() || "";

  useEffect(() => {
    if (!active || !isTauri()) return;
    let stop = false;
    setError(null);
    void tabularisEnsureStarted()
      .then((next) => {
        if (!stop) setStatus(next);
      })
      .catch((err: unknown) => {
        if (!stop) setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      stop = true;
    };
  }, [active]);

  useEffect(() => {
    if (!active || !pageReady) return;
    const theme =
      document.documentElement.getAttribute("data-theme") === "light" ? "light" : "dark";
    void syncTabularisHost(locale, theme).catch(() => undefined);
  }, [active, locale, pageReady]);

  useEffect(() => {
    if (!active) return;
    const root = document.documentElement;
    const push = () => {
      if (!pageReady) return;
      const theme = root.getAttribute("data-theme") === "light" ? "light" : "dark";
      void syncTabularisHost(locale, theme).catch(() => undefined);
    };
    const observer = new MutationObserver(push);
    observer.observe(root, { attributes: true, attributeFilter: ["data-theme"] });
    return () => observer.disconnect();
  }, [active, locale, pageReady]);

  useEffect(() => {
    if (!active || !pageReady || !queuedSql) return;
    const sql = queuedSql;
    const timer = window.setTimeout(() => {
      void deliverSqlToDatabase(sql)
        .then(() => clearQueuedDatabaseSql())
        .catch(() => undefined);
    }, 400);
    return () => window.clearTimeout(timer);
  }, [active, pageReady, queuedSql]);

  return (
    <div className="octo-pane" hidden={!active}>
      {error ? (
        <div className="settings-card" role="alert">
          <p>{tr("database.failed")}</p>
          <p>{error}</p>
          <button
            type="button"
            className="btn"
            onClick={() => {
              setError(null);
              setStatus(null);
              void tabularisEnsureStarted().then(setStatus).catch((err: unknown) => {
                setError(err instanceof Error ? err.message : String(err));
              });
            }}
          >
            {tr("database.retry")}
          </button>
        </div>
      ) : url ? (
        <EmbeddedBrowser
          url={url}
          title={title}
          locale={locale}
          instanceId="tabularis"
          active={active}
          hideChromeBar
          onLoadingChange={(loading) => setPageReady(!loading)}
        />
      ) : (
        <div className="settings-card" role="status">
          {tr("settings.database.starting")}
        </div>
      )}
    </div>
  );
}

export function DatabaseSlot({
  open,
  locale,
  title,
}: {
  open: boolean;
  locale: Locale;
  title: string;
}) {
  const keptRef = useRef(open);
  if (open) keptRef.current = true;
  if (!keptRef.current) return null;
  return <DatabasePane locale={locale} title={title} active={open} />;
}
