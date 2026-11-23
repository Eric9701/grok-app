/**
 * Env menu “Commit or push”: decide whether a message is required,
 * and recognize a push that moved nothing.
 */

export function commitPushNeedsMessage(fileCount: number): boolean {
  return fileCount > 0;
}

export function commitPushMode(fileCount: number): "commit-push" | "push" {
  return fileCount > 0 ? "commit-push" : "push";
}

/** Git prints this when the remote already has the same commits. */
export function pushOutputIsUpToDate(
  stdout?: string | null,
  stderr?: string | null,
): boolean {
  const text = `${stdout ?? ""}\n${stderr ?? ""}`.toLowerCase();
  return (
    text.includes("everything up-to-date") ||
    text.includes("everything up to date") ||
    text.includes("一切皆为最新") ||
    text.includes("已經是最新") ||
    text.includes("已经是最新")
  );
}

export function commitFailureKind(
  reason: string | null | undefined,
): "detached" | "message" | "invalid" | "other" {
  const code = (reason ?? "").trim();
  if (code === "detached_head") return "detached";
  if (code === "message_required") return "message";
  if (code === "invalid_message") return "invalid";
  return "other";
}
