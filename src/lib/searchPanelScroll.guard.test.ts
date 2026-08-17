/**
 * Structural guard: command-palette hits must live in a dedicated scrollport.
 * `.search-panel` clips to radius (`overflow: hidden` + max-height). Without
 * an inner overflow-y scroller, wheel / trackpad do nothing (#543 pattern).
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = join(__dirname, "..");

describe("search panel scrollport", () => {
  it("wraps palette hits in OverlayScroll.search-panel__results", () => {
    const src = readFileSync(join(ROOT, "app/AppWorkbench.tsx"), "utf8");
    expect(src).toMatch(
      /<OverlayScroll className="search-panel__results">/,
    );
    expect(src).toMatch(/<\/OverlayScroll>/);
    const open = src.indexOf(
      '<OverlayScroll className="search-panel__results">',
    );
    const close = src.indexOf("</OverlayScroll>", open);
    const inner = src.slice(open, close);
    expect(inner).toContain("search-panel__row");
    expect(inner).toContain("mergedSessionHits.map");
    expect(inner).toContain("data-search-idx");
    expect(inner).toContain("search-opt-");
  });

  it("wires keyboard selection through useSearchPanelNav", () => {
    const src = readFileSync(join(ROOT, "app/AppWorkbench.tsx"), "utf8");
    expect(src).toContain("useSearchPanelNav");
    expect(src).toContain("flattenSearchPanelItems");
    expect(src).toContain("searchActiveIndex");
    expect(src).toMatch(/role="combobox"/);
  });

  it("keeps results as the overflow-y scrollport under a clipping panel", () => {
    const css = readFileSync(join(ROOT, "styles/sidebar.part2.css"), "utf8");
    expect(css).toMatch(/\.search-panel\s*\{[^}]*overflow:\s*hidden/s);
    expect(css).toMatch(/\.search-panel__results\s*\{[^}]*min-height:\s*0/s);
    expect(css).toMatch(
      /\.search-panel__results \.overlay-scroll__viewport\s*\{[^}]*overflow-y:\s*auto/s,
    );
    expect(css).toMatch(/\.search-panel__row\.is-active/);
  });
});
