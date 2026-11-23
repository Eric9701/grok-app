/**
 * Environment-menu “Changes” badge.
 * Once git status for this path is known, the number is uncommitted files.
 * A clean worktree shows nothing, even if this chat still remembers edits.
 * Before that status arrives, fall back to the session summary.
 */

export type EnvChangesBadge =
  | { kind: "delta"; add: number; del: number }
  | { kind: "files"; count: number }
  | { kind: "none" };

export function envChangesBadge(input: {
  gitLoaded: boolean;
  isGit: boolean;
  dirtyCount: number;
  sessionAdd?: number;
  sessionDel?: number;
  sessionFileCount?: number;
}): EnvChangesBadge {
  if (input.gitLoaded && input.isGit) {
    return input.dirtyCount > 0
      ? { kind: "files", count: input.dirtyCount }
      : { kind: "none" };
  }
  const add = input.sessionAdd ?? 0;
  const del = input.sessionDel ?? 0;
  if (add > 0 || del > 0) return { kind: "delta", add, del };
  const files = input.sessionFileCount ?? 0;
  if (files > 0) return { kind: "files", count: files };
  return { kind: "none" };
}
