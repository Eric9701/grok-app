/**
 * First-run Atlas CLI download source.
 *
 * Presets match the enterprise intranet and public channels. "custom" is a
 * user-typed http(s) base. Host still allowlists whatever concrete URL is sent.
 */

export const ATLAS_CLI_MIRROR_INTRANET = "http://10.218.220.237:22255/atlas/cli";
export const ATLAS_CLI_MIRROR_PUBLIC = "https://atlas.myai.tech/atlas/cli";

export type AtlasCliMirrorChoice = "intranet" | "public" | "custom";

export function isAtlasCliMirrorChoice(v: string): v is AtlasCliMirrorChoice {
  return v === "intranet" || v === "public" || v === "custom";
}

/** Trim, drop trailing slashes, require http(s), reject userinfo and `..`. */
export function normalizeAtlasCliMirror(raw: string): string | null {
  const t = raw.trim().replace(/\/+$/, "");
  if (!t) return null;
  if (t.includes("@") || t.includes("..") || /\s/.test(t)) return null;
  if (!t.startsWith("https://") && !t.startsWith("http://")) return null;
  return t;
}

export function resolveAtlasCliMirror(
  choice: AtlasCliMirrorChoice,
  custom: string,
): string | null {
  if (choice === "intranet") return ATLAS_CLI_MIRROR_INTRANET;
  if (choice === "public") return ATLAS_CLI_MIRROR_PUBLIC;
  return normalizeAtlasCliMirror(custom);
}

/** Map a saved or resolved base back onto the wizard choices. Empty → intranet. */
export function classifyAtlasCliMirror(raw: string | null | undefined): {
  choice: AtlasCliMirrorChoice;
  custom: string;
} {
  const n = raw ? normalizeAtlasCliMirror(raw) : null;
  if (!n || n === ATLAS_CLI_MIRROR_INTRANET) {
    return { choice: "intranet", custom: "" };
  }
  if (n === ATLAS_CLI_MIRROR_PUBLIC) {
    return { choice: "public", custom: "" };
  }
  return { choice: "custom", custom: n };
}

export function formatAtlasCliInstallCommand(
  base: string,
  platform: "mac" | "win" | "linux" | "other",
): string {
  if (platform === "win") return `irm ${base}/install.ps1 | iex`;
  return `curl -fsSL ${base}/install.sh | bash`;
}
