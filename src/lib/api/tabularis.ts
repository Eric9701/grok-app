/** API domain: embedded Tabularis. */

import { invoke, isTauri, listen } from "./host";
import { sideBrowserEval } from "./system";
import { queueDatabaseSql } from "@/lib/tabularisStore";

export interface TabularisStatus {
  state: string;
  origin?: string | null;
  bootstrapUrl?: string | null;
  binary?: string | null;
  dataDirMode: string;
  error?: string | null;
  pendingApprovals: number;
}

export interface TabularisApproval {
  id: string;
  connectionId?: string | null;
  connectionName?: string | null;
  database?: string | null;
  query: string;
  queryKind?: string | null;
  createdAt?: string | null;
}

export function tabularisWebviewLabel(): string {
  return "resource-browser-tabularis";
}

export async function tabularisStatus(): Promise<TabularisStatus> {
  return invoke<TabularisStatus>("tabularis_status");
}

export async function tabularisEnsureStarted(): Promise<TabularisStatus> {
  return invoke<TabularisStatus>("tabularis_ensure_started");
}

export async function tabularisRestart(): Promise<TabularisStatus> {
  return invoke<TabularisStatus>("tabularis_restart");
}

export async function tabularisStop(): Promise<TabularisStatus> {
  return invoke<TabularisStatus>("tabularis_stop");
}

export async function tabularisDecideApproval(
  id: string,
  decision: "approve" | "deny",
): Promise<void> {
  await invoke("tabularis_decide_approval", { id, decision });
}

export async function tabularisPendingApprovals(): Promise<TabularisApproval[]> {
  return invoke<TabularisApproval[]>("tabularis_pending_approvals");
}

export function listenTabularis<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  return listen<T>(event, handler);
}

export async function openSqlInDatabase(sql: string): Promise<void> {
  const text = sql.trim();
  if (!text) return;
  queueDatabaseSql(text);
  if (isTauri()) {
    await tabularisEnsureStarted();
  }
  window.location.hash = "#/database";
}

export async function deliverSqlToDatabase(sql: string): Promise<void> {
  const detail = JSON.stringify({ sql });
  await sideBrowserEval(
    tabularisWebviewLabel(),
    `(() => {
      const detail = ${detail};
      if (location.pathname.includes("/editor")) {
        window.dispatchEvent(new CustomEvent("tabularis-host:open-sql", { detail }));
        return;
      }
      sessionStorage.setItem("tabularis-host-sql", JSON.stringify(detail));
      const url = new URL(location.href);
      const parts = url.pathname.split("/").filter(Boolean);
      const leaf = parts[parts.length - 1] || "";
      if (/^(connections|settings|mcp)/.test(leaf)) parts.pop();
      parts.push("editor");
      url.pathname = "/" + parts.join("/");
      url.search = "";
      url.hash = "";
      location.assign(url.toString());
    })()`,
  );
}

export async function syncTabularisHost(locale: string, theme: "light" | "dark"): Promise<void> {
  const detail = JSON.stringify({ locale, theme });
  await sideBrowserEval(
    tabularisWebviewLabel(),
    `window.dispatchEvent(new CustomEvent("tabularis-host:sync", { detail: ${detail} }))`,
  );
}
