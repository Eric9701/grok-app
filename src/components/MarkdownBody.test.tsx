import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MarkdownBody } from "./MarkdownBody";

describe("MarkdownBody http(s) links", () => {
  it("renders absolute http(s) links without target=_blank", () => {
    const html = renderToStaticMarkup(
      <MarkdownBody>{"See [docs](https://example.com/path) please."}</MarkdownBody>,
    );
    expect(html).toContain('href="https://example.com/path"');
    expect(html).toContain("md-body__link");
    expect(html).toContain("noreferrer");
    expect(html).not.toContain('target="_blank"');
    expect(html).toContain("docs");
  });

  it("leaves hash anchors as in-page links", () => {
    const html = renderToStaticMarkup(
      <MarkdownBody>{"Jump [here](#section)"}</MarkdownBody>,
    );
    expect(html).toContain('href="#section"');
    expect(html).not.toContain("md-body__link");
  });

  it("renders $$LaTeX$$ with KaTeX", () => {
    const html = renderToStaticMarkup(
      <MarkdownBody>{String.raw`Area is $$\tau=0.53$$.`}</MarkdownBody>,
    );
    expect(html).toContain("katex");
    expect(html).not.toContain("$$\\tau");
  });
});
