/**
 * CLI picked on the unsaved new-chat composer.
 * `undefined` means the user has not picked, so the new chat keeps the
 * desktop default. `""` is an explicit "Current Atlas".
 */

let override: string | undefined;
const listeners = new Set<() => void>();

export function draftRuntimeOverride(): string | undefined {
  return override;
}

export function resolveDraftProfileId(
  picked: string | undefined,
  desktopDefault: string,
): string {
  return picked !== undefined ? picked : desktopDefault;
}

export function setDraftRuntimeOverride(profileId: string): void {
  if (override === profileId) return;
  override = profileId;
  for (const listener of listeners) listener();
}

export function clearDraftRuntimeOverride(): void {
  if (override === undefined) return;
  override = undefined;
  for (const listener of listeners) listener();
}

export function subscribeDraftRuntime(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
