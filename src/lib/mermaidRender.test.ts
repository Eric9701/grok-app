/**
 * @vitest-environment jsdom
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const renderMock = vi.fn(async () => ({
  svg: '<svg xmlns="http://www.w3.org/2000/svg"><text>ok</text></svg>',
}));
const initializeMock = vi.fn();

vi.mock("mermaid", () => ({
  default: {
    initialize: initializeMock,
    render: renderMock,
  },
}));

describe("mermaidRender", () => {
  beforeEach(() => {
    renderMock.mockClear();
    initializeMock.mockClear();
    document.documentElement.removeAttribute("data-theme");
    vi.resetModules();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("accepts mermaid fence language tags case-insensitively", async () => {
    const { isMermaidLanguage } = await import("./mermaidRender");
    expect(isMermaidLanguage("mermaid")).toBe(true);
    expect(isMermaidLanguage("Mermaid")).toBe(true);
    expect(isMermaidLanguage("language-mermaid")).toBe(true);
    expect(isMermaidLanguage("ts")).toBe(false);
    expect(isMermaidLanguage(undefined)).toBe(false);
  });

  it("reads light/dark from html data-theme and defaults to dark", async () => {
    const { documentThemeMode } = await import("./mermaidRender");
    expect(documentThemeMode()).toBe("dark");
    document.documentElement.setAttribute("data-theme", "light");
    expect(documentThemeMode()).toBe("light");
    document.documentElement.setAttribute("data-theme", "dark");
    expect(documentThemeMode()).toBe("dark");
  });

  it("returns an svg string for a simple flowchart", async () => {
    const { renderMermaidSvg } = await import("./mermaidRender");
    const svg = await renderMermaidSvg("flowchart LR\n  A-->B", "dark");
    expect(svg).toMatch(/<svg[\s>]/i);
    expect(initializeMock).toHaveBeenCalled();
    expect(renderMock).toHaveBeenCalled();
    const call = renderMock.mock.calls[0] as unknown as [string, string];
    expect(call[1]).toContain("flowchart LR");
  });

  it("rejects empty source", async () => {
    const { renderMermaidSvg } = await import("./mermaidRender");
    await expect(renderMermaidSvg("   ", "dark")).rejects.toThrow(/empty/i);
    expect(renderMock).not.toHaveBeenCalled();
  });

  it("initializes mermaid with error rendering suppressed", async () => {
    const { renderMermaidSvg } = await import("./mermaidRender");
    await renderMermaidSvg("flowchart LR\n  A-->B", "dark");
    const config = initializeMock.mock.calls[0]?.[0] as Record<string, unknown>;
    expect(config.suppressErrorRendering).toBe(true);
  });

  it("rejects mermaid's built-in error diagram svg", async () => {
    renderMock.mockImplementationOnce(async () => ({
      svg: '<svg xmlns="http://www.w3.org/2000/svg"><style>.error-text{fill:red}</style><text class="error-text">Syntax error in text</text><text>mermaid version 11.17.2</text></svg>',
    }));
    const { renderMermaidSvg } = await import("./mermaidRender");
    await expect(renderMermaidSvg("not a diagram", "dark")).rejects.toThrow(
      /error diagram/i,
    );
  });

  it("accepts a diagram whose stylesheet defines an error-text class", async () => {
    renderMock.mockImplementationOnce(async () => ({
      svg: '<svg xmlns="http://www.w3.org/2000/svg"><style>.error-text{fill:red}</style><text>A</text></svg>',
    }));
    const { renderMermaidSvg } = await import("./mermaidRender");
    const svg = await renderMermaidSvg("flowchart LR\n  A-->B", "dark");
    expect(svg).toMatch(/<svg[\s>]/i);
  });

  it("removes the leaked temp element when a render fails", async () => {
    renderMock.mockImplementationOnce(async () => {
      const leaked = document.createElement("div");
      leaked.id = "dgrok-mermaid-1";
      document.body.appendChild(leaked);
      throw new Error("Parse error on line 2");
    });
    const { renderMermaidSvg } = await import("./mermaidRender");
    await expect(renderMermaidSvg("flowchart LR\n  ???", "dark")).rejects.toThrow(
      /parse error/i,
    );
    expect(document.getElementById("dgrok-mermaid-1")).toBeNull();
  });
});
