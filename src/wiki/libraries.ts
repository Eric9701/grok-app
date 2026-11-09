import type { WikiLibraryRecord } from "@/lib/api";

export type WikiLibrary = WikiLibraryRecord;

export function wikiPathKey(path: string) {
  const slash = path.trim().replace(/\\/g, "/").replace(/\/+$/, "");
  return slash.length > 1 && slash[1] === ":" ? slash.toLowerCase() : slash;
}

export function isAbsoluteLibraryPath(path: string) {
  const text = path.trim();
  if (!text || /[\n\r\0]/.test(text) || text.includes("..")) return false;
  return /^[A-Za-z]:[\\/]/.test(text) || text.startsWith("/");
}

export function libraryFolderName(path: string) {
  const slash = path.trim().replace(/\\/g, "/").replace(/\/+$/, "");
  return (slash.split("/").pop() || "wiki").slice(0, 80);
}
