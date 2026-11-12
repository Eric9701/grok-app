import * as api from "@/lib/api";
import { displayLibraryPath, libraryFolderName, wikiPathKey, type WikiLibrary } from "./libraries";

export async function createWikiLibrary(input: {
  parent: string;
  name: string;
  template: string;
}): Promise<WikiLibrary[]> {
  const path = displayLibraryPath(
    await api.wikiScaffold(input.parent, input.name, input.template),
  );
  const settings = await api.settingsGet();
  const rows = (settings.wikiLibraries ?? []).map((row) => ({
    ...row,
    path: displayLibraryPath(row.path),
  }));
  if (rows.some((row) => wikiPathKey(row.path) === wikiPathKey(path))) {
    const err = new Error("wiki_duplicate");
    throw err;
  }
  const next: WikiLibrary[] = [
    ...rows,
    {
      id: globalThis.crypto?.randomUUID?.() ?? `wiki-${Date.now()}`,
      name: input.name.trim() || libraryFolderName(path),
      path,
      agentDefault: rows.every((row) => !row.agentDefault),
    },
  ];
  const saved = await api.settingsSet({ ...settings, wikiLibraries: next });
  return saved.wikiLibraries ?? next;
}
