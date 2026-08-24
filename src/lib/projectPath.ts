/**
 * Project folder path health (D05).
 * Backend sets `pathOk` on list by re-checking `is_dir` — never invent it in the UI.
 */

/** True when Host reported the project path is missing / not a directory. */
export function isProjectPathMissing(
  pathOk: boolean | undefined | null,
): boolean {
  return pathOk === false;
}

/** True when `path` lives on an OpenSSH Host, not this machine. */
export function isSshRemoteProject(
  project: { sshAlias?: string | null } | null | undefined,
): boolean {
  return !!project?.sshAlias?.trim();
}

/**
 * Warm-connect may spawn `grok agent stdio`. SSH remotes spawn through OpenSSH
 * with the remote path as cwd — they are warmable when trusted. Local folders
 * still require a real directory. Null project (orphan) stays warmable.
 */
export function isProjectWarmable(
  project: {
    trusted?: boolean;
    pathOk?: boolean | null;
    sshAlias?: string | null;
  } | null,
): boolean {
  if (!project) return true;
  if (isSshRemoteProject(project)) return !!project.trusted;
  return !!project.trusted && !isProjectPathMissing(project.pathOk);
}

/**
 * SSH projects created on open must not duplicate the watching remote rail.
 * When that host is not watching, keep the folder so imported chats stay reachable.
 */
export function hideSshProjectInLocalTree(
  project: { sshAlias?: string | null },
  watchingAliases: readonly string[],
): boolean {
  const alias = project.sshAlias?.trim();
  if (!alias) return false;
  return watchingAliases.some((a) => a.trim() === alias);
}
