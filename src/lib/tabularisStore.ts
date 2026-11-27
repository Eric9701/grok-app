/** Pending Tabularis approvals and a SQL draft waiting for the database pane. */

import { useSyncExternalStore } from "react";
import type { TabularisApproval } from "@/lib/api/tabularis";

let approvals: TabularisApproval[] = [];
let queuedSql: string | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function pushTabularisApproval(row: TabularisApproval) {
  approvals = [...approvals.filter((item) => item.id !== row.id), row].slice(-20);
  emit();
}

export function dismissTabularisApproval(id: string) {
  approvals = approvals.filter((item) => item.id !== id);
  emit();
}

export function replaceTabularisApprovals(rows: TabularisApproval[]) {
  approvals = rows;
  emit();
}

export function useTabularisApprovals(): TabularisApproval[] {
  return useSyncExternalStore(
    subscribe,
    () => approvals,
    () => approvals,
  );
}

export function queueDatabaseSql(sql: string) {
  queuedSql = sql;
  emit();
}

export function useQueuedDatabaseSql(): string | null {
  return useSyncExternalStore(
    subscribe,
    () => queuedSql,
    () => queuedSql,
  );
}

export function clearQueuedDatabaseSql() {
  if (!queuedSql) return;
  queuedSql = null;
  emit();
}
