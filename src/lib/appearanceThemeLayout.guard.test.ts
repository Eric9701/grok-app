/**
 * Appearance · Theme layout: presets rail left, look stack right.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const section = readFileSync(
  resolve(__dirname, "../components/settings/AppearanceSection.tsx"),
  "utf8",
);
const css = readFileSync(
  resolve(__dirname, "../styles/settings.part3.css"),
  "utf8",
);
const wallpaperCss = readFileSync(
  resolve(__dirname, "../styles/settings.part4.css"),
  "utf8",
);

describe("appearance theme layout", () => {
  it("puts presets in a sticky left rail before the look stack", () => {
    const layout = section.indexOf("settings-appearance-theme-layout");
    const rail = section.indexOf("<SkinPresetsCard");
    const theme = section.indexOf('id="settings-anchor-theme"');
    const skin = section.indexOf('id="settings-anchor-skin"');
    const wallpaper = section.indexOf('id="settings-anchor-wallpaper"');
    const chrome = section.indexOf("<AppearanceChromeCard");
    expect(layout).toBeGreaterThanOrEqual(0);
    expect(rail).toBeGreaterThan(layout);
    expect(theme).toBeGreaterThan(rail);
    expect(skin).toBeGreaterThan(theme);
    expect(wallpaper).toBeGreaterThan(skin);
    expect(chrome).toBeGreaterThan(wallpaper);
    expect(section).not.toContain("settings-appearance-duo");
    expect(section.match(/<SkinPresetsCard/g)?.length).toBe(1);
  });

  it("uses dual-column scroll without compressing card content height", () => {
    expect(css).toMatch(
      /\.settings-appearance-theme-layout\s*\{[^}]*grid-template-columns:\s*minmax\(200px, 240px\) minmax\(0, 1fr\)/s,
    );
    expect(css).toMatch(
      /\.settings-appearance-theme-layout__rail,\s*\.settings-appearance-theme-layout__main\s*\{[^}]*overflow-y:\s*auto/s,
    );
    expect(css).toMatch(
      /\.settings-appearance-theme-layout__main\s*>\s*\.settings-card\s*\{[^}]*flex:\s*0 0 auto/s,
    );
    expect(wallpaperCss).toMatch(/\.settings-wallpaper--split/);
    expect(wallpaperCss).toMatch(
      /\.settings-wallpaper--split\s*\{[^}]*align-items:\s*start/s,
    );
    expect(wallpaperCss).toMatch(
      /\.settings-wallpaper--split \.settings-wallpaper__preview\s*\{[^}]*height:\s*auto/s,
    );
    expect(wallpaperCss).not.toMatch(
      /\.settings-wallpaper--split \.settings-wallpaper__preview\s*\{[^}]*height:\s*100%/s,
    );
  });
});
