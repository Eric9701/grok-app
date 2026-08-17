/**
 * Structural guard: assistant conclusion text must not paint over a sibling
 * (tool stdout / process speech). The #667/#672 Working-rail crush used
 * flex-shrink + overflow:visible; the same hole one level up (timeline /
 * answer / expand-body) stacked two bodies in one box.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = join(__dirname, "../components/lobe-chat");

function css(file: string): string {
  return readFileSync(join(ROOT, file), "utf8");
}

describe("chat answer overlap guard", () => {
  it("keeps transcript rows from flex-shrinking inside .lobe-chat__inner", () => {
    const src = css("lobe-chat.part1.css");
    expect(src).toMatch(/\.lobe-chat-item\s*\{[^}]*flex-shrink:\s*0/s);
  });

  it("keeps the conclusion markdown from shrinking under the work fold", () => {
    const src = css("lobe-chat.part2.css");
    expect(src).toMatch(/\.chat-md--answer\s*\{[^}]*flex-shrink:\s*0/s);
  });

  it("scopes expand-body overflow:visible to a capped/virtual scroller", () => {
    const src = css("lobe-chat.part2.css");
    // Bare `.grok-act__expand-body { overflow: visible }` lets bash stdout
    // paint over the answer when the fold height is short.
    expect(src).not.toMatch(
      /(?:^|\n)\.grok-act__expand-body\s*\{[^}]*overflow:\s*visible/s,
    );
    expect(src).toMatch(
      /\.grok-act__steps--(?:capped|virtual)\s+\.grok-act__expand-body\s*\{[^}]*overflow:\s*visible/s,
    );
  });

  it("gives answer paragraphs an explicit unitless line-height", () => {
    const src = css("lobe-chat.part1.css");
    expect(src).toMatch(
      /\.chat-md p\s*\{[^}]*line-height:\s*var\(--chat-lh\)/s,
    );
  });
});
