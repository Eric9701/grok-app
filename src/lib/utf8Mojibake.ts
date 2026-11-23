/**
 * Undo provider text whose UTF-8 bytes were stored as characters.
 *
 * The host ANSI strip used `byte as char` (U+00xx). Some Windows decoders
 * instead map 0x80–0x9F with Windows-1252 (`…` `‡` `š`). Either form shows
 * up as `å·²` / `è¶…` in the original-exception box. Real text stays.
 */

const CP1252_BYTE: Record<number, number> = {
  0x20ac: 0x80,
  0x201a: 0x82,
  0x0192: 0x83,
  0x201e: 0x84,
  0x2026: 0x85,
  0x2020: 0x86,
  0x2021: 0x87,
  0x02c6: 0x88,
  0x2030: 0x89,
  0x0160: 0x8a,
  0x2039: 0x8b,
  0x0152: 0x8c,
  0x017d: 0x8e,
  0x2018: 0x91,
  0x2019: 0x92,
  0x201c: 0x93,
  0x201d: 0x94,
  0x2022: 0x95,
  0x2013: 0x96,
  0x2014: 0x97,
  0x02dc: 0x98,
  0x2122: 0x99,
  0x0161: 0x9a,
  0x203a: 0x9b,
  0x0153: 0x9c,
  0x017e: 0x9e,
  0x0178: 0x9f,
};

function encodeSingleByte(
  text: string,
  extra: Record<number, number> | null,
): Uint8Array | null {
  const bytes: number[] = [];
  let hasHigh = false;
  for (const ch of text) {
    const cp = ch.codePointAt(0) ?? 0;
    if (cp <= 0x7f) {
      bytes.push(cp);
      continue;
    }
    if (cp >= 0xa0 && cp <= 0xff) {
      hasHigh = true;
      bytes.push(cp);
      continue;
    }
    if (cp >= 0x80 && cp <= 0x9f && !extra) {
      hasHigh = true;
      bytes.push(cp);
      continue;
    }
    const mapped = extra?.[cp];
    if (mapped == null) return null;
    hasHigh = true;
    bytes.push(mapped);
  }
  if (!hasHigh) return null;
  return Uint8Array.from(bytes);
}

function acceptUtf8(bytes: Uint8Array, original: string): string | null {
  let decoded: string;
  try {
    decoded = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
  if (decoded === original) return null;
  if (!/\p{Script=Han}/u.test(decoded)) return null;
  if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(decoded)) return null;
  return decoded;
}

export function repairLatin1Utf8Mojibake(text: string): string {
  if (!text) return text;
  const latin1 = encodeSingleByte(text, null);
  if (latin1) {
    const restored = acceptUtf8(latin1, text);
    if (restored) return restored;
  }
  const cp1252 = encodeSingleByte(text, CP1252_BYTE);
  if (cp1252) {
    const restored = acceptUtf8(cp1252, text);
    if (restored) return restored;
  }
  return text;
}
