/** Non-blocking SQL approval card plus the host event bridge. */

import { useEffect } from "react";
import { createT, type Locale } from "@/i18n";
import { setDraft } from "@/lib/composerDraftStore";
import {
  listenTabularis,
  tabularisDecideApproval,
  tabularisPendingApprovals,
  type TabularisApproval,
} from "@/lib/api/tabularis";
import { isTauri } from "@/lib/api/host";
import {
  dismissTabularisApproval,
  pushTabularisApproval,
  replaceTabularisApprovals,
  useTabularisApprovals,
} from "@/lib/tabularisStore";
import type { WorkbenchHashPane } from "@/lib/workbenchHash";

function draftFromPayload(payload: Record<string, unknown>): string {
  const sql = typeof payload.sql === "string" ? payload.sql : "";
  const name = typeof payload.connectionName === "string" ? payload.connectionName : "";
  const database = typeof payload.database === "string" ? payload.database : "";
  const error = typeof payload.error === "string" ? payload.error : "";
  const header = [name, database].filter(Boolean).join(" / ");
  const parts = [
    header ? `SQL (${header})` : "SQL",
    sql ? "```sql\n" + sql.trim() + "\n```" : "",
    error,
  ].filter(Boolean);
  return parts.join("\n\n");
}

export function DatabaseHostBridge({
  locale,
  mainPane,
}: {
  locale: Locale;
  mainPane: WorkbenchHashPane;
}) {
  const tr = createT(locale);
  const pending = useTabularisApprovals();

  useEffect(() => {
    if (!isTauri()) return;
    let stopApproval = () => {};
    let stopChat = () => {};
    let closed = false;
    void tabularisPendingApprovals()
      .then((rows) => {
        if (!closed) replaceTabularisApprovals(rows);
      })
      .catch(() => undefined);
    void listenTabularis<TabularisApproval>("tabularis://approval", (row) => {
      pushTabularisApproval(row);
    }).then((stop) => {
      if (closed) stop();
      else stopApproval = stop;
    });
    void listenTabularis<Record<string, unknown>>("tabularis://send-to-chat", (payload) => {
      const text = draftFromPayload(payload ?? {});
      if (text.trim()) setDraft(text);
      window.location.hash = "#/chat";
    }).then((stop) => {
      if (closed) stop();
      else stopChat = stop;
    });
    return () => {
      closed = true;
      stopApproval();
      stopChat();
    };
  }, []);

  if (mainPane === "database" || pending.length === 0) return null;
  const current = pending[pending.length - 1];
  const decide = (decision: "approve" | "deny") => {
    dismissTabularisApproval(current.id);
    void tabularisDecideApproval(current.id, decision).catch(() => {
      pushTabularisApproval(current);
    });
  };

  return (
    <div className="settings-card" role="status">
      <div className="settings-row__label">{tr("database.approvalTitle")}</div>
      <pre className="md-body__pre">
        <code>{current.query}</code>
      </pre>
      <div className="settings-row">
        <button type="button" className="btn" onClick={() => decide("approve")}>
          {tr("database.approvalAllow")}
        </button>
        <button type="button" className="btn btn--ghost" onClick={() => decide("deny")}>
          {tr("database.approvalDeny")}
        </button>
        <button
          type="button"
          className="btn btn--ghost"
          onClick={() => {
            window.location.hash = "#/database";
          }}
        >
          {tr("database.approvalOpen")}
        </button>
      </div>
    </div>
  );
}
