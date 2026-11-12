import type { WikiLibraryRecord } from "@/lib/api";

export type WikiLibrary = WikiLibraryRecord;

/** Windows canonicalize prefixes a path with `\\?\`. People should see `E:\...`. */
export function displayLibraryPath(path: string) {
  const text = path.trim();
  const unc = text.match(/^\\\\\?\\UNC\\([\s\S]*)$/i);
  if (unc) return `\\\\${unc[1]}`;
  const drive = text.match(/^\\\\\?\\([A-Za-z]:[\s\S]*)$/);
  if (drive) return drive[1];
  const slashUnc = text.match(/^\/\/\?\/UNC\/([\s\S]*)$/i);
  if (slashUnc) return `//${slashUnc[1]}`;
  const slashDrive = text.match(/^\/\/\?\/([A-Za-z]:[\s\S]*)$/);
  if (slashDrive) return slashDrive[1];
  return text;
}

export function wikiPathKey(path: string) {
  const slash = displayLibraryPath(path).replace(/\\/g, "/").replace(/\/+$/, "");
  return slash.length > 1 && slash[1] === ":" ? slash.toLowerCase() : slash;
}

export function isAbsoluteLibraryPath(path: string) {
  const text = displayLibraryPath(path);
  if (!text || /[\n\r\0]/.test(text) || text.includes("..")) return false;
  return /^[A-Za-z]:[\\/]/.test(text) || text.startsWith("/");
}

export function libraryFolderName(path: string) {
  const slash = displayLibraryPath(path).replace(/\\/g, "/").replace(/\/+$/, "");
  return (slash.split("/").pop() || "wiki").slice(0, 80);
}
