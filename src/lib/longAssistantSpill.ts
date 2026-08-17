/**
 * Spill oversized assistant bubbles to a .txt file card.
 *
 * Virtualizing the transcript by *row count* does not help a single huge
 * reply: ReactMarkdown still re-parses the whole string. Preview the head
 * in-chat and persist the full body via save_temp_attachment.
 */

/** Paint a file card / preview instead of the full markdown tree. */
export const LONG_ASSISTANT_SPILL_CHARS = 8000;

/** Characters kept in the live/preview markdown parse. */
export const LONG_ASSISTANT_PREVIEW_CHARS = 800;

const spillPathCache = new Map<string, string>();

export function shouldSpillLongAssistant(length: number): boolean {
  return length >= LONG_ASSISTANT_SPILL_CHARS;
}

/**
 * First ~previewChars, snapped back to a nearby newline so we rarely split
 * a fence or table row. Always shorter than `src` when spilling.
 */
export function previewLongAssistant(
  src: string,
  max: number = LONG_ASSISTANT_PREVIEW_CHARS,
): string {
  if (src.length <= max) return src;
  const slice = src.slice(0, max);
  const nl = slice.lastIndexOf("\n");
  const cut = nl >= Math.floor(max * 0.6) ? nl : max;
  return src.slice(0, cut).trimEnd() + "\n";
}

export function spillCacheKey(messageId: string, length: number): string {
  return `${messageId}:${length}`;
}

export function getCachedSpillPath(key: string): string | undefined {
  return spillPathCache.get(key);
}

export function setCachedSpillPath(key: string, path: string): void {
  if (!key || !path) return;
  spillPathCache.set(key, path);
}

/** Test-only. */
export function clearSpillPathCache(): void {
  spillPathCache.clear();
}

/** UTF-8 → standard base64 for save_temp_attachment. */
export function utf8ToBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export function safeSpillFileStem(messageId: string): string {
  const raw = (messageId || "reply").replace(/[^a-zA-Z0-9_-]+/g, "");
  const stem = raw.slice(0, 12) || "reply";
  return `assistant-${stem}`;
}
