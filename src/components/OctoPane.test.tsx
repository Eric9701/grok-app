/** @vitest-environment jsdom */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import "@/test/jsdomStubs";
import { OCTO_URL, OctoSlot } from "./OctoPane";

afterEach(() => {
  cleanup();
});

describe("OctoSlot", () => {
  it("hides the address and keeps refresh plus open-externally", () => {
    render(<OctoSlot open locale="zh" title="Octo" />);

    expect(document.querySelector(".embedded-browser__url")).toBeNull();
    expect(screen.queryByText(OCTO_URL)).toBeNull();
    expect(screen.getByTitle("刷新")).toBeTruthy();
    expect(screen.getByTitle("用系统应用打开")).toBeTruthy();
  });

  it("keeps the same frame when the menu switches away and back", () => {
    const { rerender } = render(<OctoSlot open locale="zh" title="Octo" />);
    const frame = document.querySelector("iframe");
    expect(frame).toBeTruthy();
    const src = frame?.getAttribute("src");

    rerender(<OctoSlot open={false} locale="zh" title="Octo" />);
    expect(document.querySelector(".octo-pane")).toHaveProperty("hidden", true);
    expect(document.querySelector("iframe")).toBe(frame);
    expect(frame?.getAttribute("src")).toBe(src);

    rerender(<OctoSlot open locale="zh" title="Octo" />);
    expect(document.querySelector(".octo-pane")?.hasAttribute("hidden")).toBe(false);
    expect(document.querySelector("iframe")).toBe(frame);
    expect(frame?.getAttribute("src")).toBe(src);
  });
});
