/** Turns a host rejection into a wiki sentence. Tauri invoke rejects with a string. */

import type { MessageKey, Vars } from "@/i18n";

type Tr = (key: MessageKey, vars?: Vars) => string;

function hostText(err: unknown): string {
  if (typeof err === "string") return err.trim();
  if (err instanceof Error) return err.message.trim();
  return "";
}

export function formatWikiNotice(tr: Tr, err: unknown, source = ""): string {
  const text = hostText(err);
  const code = text.split(":")[0]?.trim() ?? "";
  switch (code) {
    case "wiki_path":
      return tr("wiki.errPath");
    case "wiki_missing":
      return tr("wiki.errMissing", { target: source.trim() || text });
    case "wiki_project":
      return tr("wiki.errProject");
    case "wiki_no_pages":
      return tr("wiki.errNoPages");
    case "wiki_model_missing":
      return tr("wiki.errModel");
    case "wiki_model_unknown":
      return tr("wiki.errModelUnknown", {
        model: text.slice(code.length).replace(/^:/, "").trim() || source,
      });
    case "wiki_complete_failed":
      return tr("wiki.errFailed", {
        detail: text.slice(code.length).replace(/^:/, "").trim().slice(0, 180) || "unknown",
      });
    case "wiki_cli_missing":
      return tr("wiki.errCli");
    case "wiki_complete_empty":
      return tr("wiki.errEmpty");
    case "wiki_complete_spawn":
    case "wiki_complete_join": {
      const detail = text.slice(code.length).replace(/^:/, "").trim().slice(0, 180);
      return tr("wiki.errSpawn", { detail: detail ? ` ${detail}` : "" });
    }
    case "wiki_complete_timeout":
      return tr("wiki.errTimeout");
    case "wiki_overwrite":
      return tr("wiki.errOverwrite");
    default:
      return tr("wiki.errFailed", { detail: (text || "unknown").slice(0, 180) });
  }
}
