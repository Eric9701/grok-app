/** Display helpers for the sidebar SSH remote session rail. */

export const SSH_REMOTE_PAGE_SIZE = 20;

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function looksLikeSessionId(value: string): boolean {
  return UUID_RE.test(value.trim());
}

/** First non-empty line, collapsed whitespace, capped. */
export function firstSentenceTitle(raw: string, max = 48): string {
  const line =
    raw
      .split(/\r?\n/)
      .map((l) => l.trim())
      .find((l) => l.length > 0) ?? "";
  const collapsed = line.split(/\s+/).join(" ").trim();
  if (!collapsed) return "";
  const chars = [...collapsed];
  if (chars.length <= max) return collapsed;
  return `${chars.slice(0, max - 1).join("")}…`;
}

export function cwdBasename(cwd: string): string {
  const parts = cwd.replace(/\\/g, "/").split("/").filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

export function remoteTitleKey(alias: string, id: string): string {
  return `${alias}:${id}`;
}

export function remoteSessionLabel(opts: {
  title: string;
  cwd: string;
  custom?: string | null;
  untitled: string;
}): string {
  const custom = opts.custom?.trim();
  if (custom) return custom;
  const fromRemote = firstSentenceTitle(opts.title);
  if (fromRemote && !looksLikeSessionId(fromRemote)) return fromRemote;
  const base = cwdBasename(opts.cwd);
  if (base && !looksLikeSessionId(base)) return base;
  return opts.untitled;
}

export function remotePathTip(alias: string, cwd: string): string {
  const path = cwd.trim();
  if (!path) return alias;
  return `${alias} · ${path}`;
}

export function remainingRemoteCount(total: number, loaded: number): number {
  return Math.max(0, total - loaded);
}
