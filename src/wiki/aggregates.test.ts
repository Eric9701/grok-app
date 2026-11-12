import { describe, expect, it } from "vitest";
import {
  buildIngestLog,
  recentPageLines,
  updateRecentIndex,
  updateRecentOverview,
} from "./aggregates";

const page = {
  path: "wiki/concepts/pump.md",
  content: "---\ntitle: 水泵\n---\n\n# 水泵\n",
};

describe("wiki aggregates", () => {
  it("keeps the existing index and lists a new page once", () => {
    const index = "# 目录\n\n还没有页面。\n";
    const lines = recentPageLines(index, [page]);
    const next = updateRecentIndex(index, lines);
    expect(next).toContain("# 目录");
    expect(next).toContain("还没有页面。");
    expect(next).toContain("- [[concepts/pump]] — 水泵");
    expect(recentPageLines(next, [page])).toEqual([]);
  });

  it("keeps the overview body and replaces only the recent block", () => {
    const overview = "# 概览\n\n还没有概览。\n";
    const first = updateRecentOverview(overview, ["- [[concepts/pump]] — 水泵"]);
    const second = updateRecentOverview(first, ["- [[concepts/valve]] — 阀门"]);
    expect(second).toContain("# 概览");
    expect(second).toContain("还没有概览。");
    expect(second.match(/llm-wiki:recent-start/g)).toHaveLength(1);
    expect(second.indexOf("阀门")).toBeLessThan(second.indexOf("水泵"));
  });

  it("appends one ingest line under the existing log", () => {
    const next = buildIngestLog("# 日志\n", "raw/sources/note.md", "2026-11-11");
    expect(next.startsWith("# 日志")).toBe(true);
    expect(next).toContain("## [2026-11-11] ingest | raw/sources/note.md");
  });
});
