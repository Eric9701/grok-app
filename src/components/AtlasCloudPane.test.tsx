/** @vitest-environment jsdom */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import "@/test/jsdomStubs";
import { AtlasCloudPane } from "@/components/AtlasCloudPane";
import { createT } from "@/i18n";

afterEach(() => {
  cleanup();
});

describe("AtlasCloudPane", () => {
  it("hides the address and keeps the page when the menu switches", () => {
    const tr = createT("zh");
    const { rerender } = render(
      <AtlasCloudPane
        locale="zh"
        title="Atlas云端"
        tr={tr}
        active
        siteUrl="https://cloud.example/"
      />,
    );

    const frame = document.querySelector("iframe");
    expect(frame?.getAttribute("src")).toContain("https://cloud.example/");
    expect(document.querySelector(".embedded-browser__url")).toBeNull();
    expect(screen.queryByText("https://cloud.example/")).toBeNull();
    expect(screen.getByTitle("刷新")).toBeTruthy();
    expect(screen.getByTitle("用系统应用打开")).toBeTruthy();

    rerender(
      <AtlasCloudPane
        locale="zh"
        title="Atlas云端"
        tr={tr}
        active={false}
        siteUrl="https://cloud.example/"
      />,
    );
    expect(document.querySelector(".atlas-cloud-pane")).toHaveProperty("hidden", true);
    expect(document.querySelector("iframe")).toBe(frame);

    rerender(
      <AtlasCloudPane
        locale="zh"
        title="Atlas云端"
        tr={tr}
        active
        siteUrl="https://cloud.example/"
      />,
    );
    expect(document.querySelector(".atlas-cloud-pane")?.hasAttribute("hidden")).toBe(false);
    expect(document.querySelector("iframe")).toBe(frame);
  });

  it("asks for a site when the address is empty", () => {
    render(
      <AtlasCloudPane
        locale="zh"
        title="Atlas云端"
        tr={createT("zh")}
        active
        siteUrl=""
      />,
    );
    expect(screen.getByText(/还没有填写 Atlas 云端站点/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "打开设置" })).toBeTruthy();
    expect(document.querySelector("iframe")).toBeNull();
  });
});
