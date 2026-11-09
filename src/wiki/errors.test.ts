import { describe, expect, it } from "vitest";
import { formatWikiNotice } from "./errors";

function tr(key: string, vars?: Record<string, string | number | undefined | null>) {
  return vars ? `${key}:${JSON.stringify(vars)}` : key;
}

describe("formatWikiNotice", () => {
  it("names a missing source instead of the empty page list", () => {
    expect(formatWikiNotice(tr, "wiki_missing", "raw/sources/note.md")).toBe(
      'wiki.errMissing:{"target":"raw/sources/note.md"}',
    );
    expect(formatWikiNotice(tr, new Error("wiki_no_pages"))).toBe("wiki.errNoPages");
    expect(formatWikiNotice(tr, "wiki_complete_timeout")).toBe("wiki.errTimeout");
    expect(formatWikiNotice(tr, "wiki_complete_spawn:filename too long")).toBe(
      'wiki.errSpawn:{"detail":" filename too long"}',
    );
    expect(formatWikiNotice(tr, "wiki_model_unknown:arch-minimax")).toBe(
      'wiki.errModelUnknown:{"model":"arch-minimax"}',
    );
  });

  it("keeps an unknown host string visible", () => {
    expect(formatWikiNotice(tr, "command wiki_complete not found")).toBe(
      'wiki.errFailed:{"detail":"command wiki_complete not found"}',
    );
  });
});
