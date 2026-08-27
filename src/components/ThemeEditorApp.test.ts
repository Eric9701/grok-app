import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const app = readFileSync(resolve(__dirname, "./ThemeEditorApp.tsx"), "utf8");
const rust = readFileSync(
  resolve(__dirname, "../../src-tauri/src/theme_editor_window.rs"),
  "utf8",
);
const css = readFileSync(resolve(__dirname, "../styles/settings.part3.css"), "utf8");

describe("theme editor OS window", () => {
  it("uses main-window chrome: Overlay on mac, frameless caption on Win/Linux", () => {
    expect(rust).toContain("THEME_EDITOR_WINDOW_LABEL");
    expect(rust).toContain("TitleBarStyle::Overlay");
    expect(rust).toContain("hidden_title(true)");
    expect(rust).toContain("decorations(false)");
    expect(rust).toContain("attach_webview_keyboard_focus");
    expect(app).toContain("WindowControls");
    expect(app).toContain("has-custom-chrome");
    expect(app).toContain("data-tauri-drag-region");
    expect(css).toContain(".theme-editor-shell__chrome");
    expect(css).toContain("-webkit-app-region: drag");
  });

  it("does not boot the workbench shell", () => {
    expect(app).not.toContain("AppWorkbench");
    expect(app).toContain("AppearanceSection");
  });

  it("keeps the editor plate solid — no wallpaper scrim on the settings window", () => {
    const skins = readFileSync(
      resolve(__dirname, "../styles/skins.css"),
      "utf8",
    );
    expect(skins).toMatch(
      /html\[data-theme-editor-shell\]\[data-wallpaper="1"\][^{]*\{[^}]*--wallpaper-theme-scrim-opacity:\s*0/s,
    );
    expect(skins).toMatch(
      /html\[data-theme-editor-shell\]\[data-wallpaper="1"\] \.app-shell::after\s*\{[^}]*content:\s*none\s*!important/s,
    );
    expect(skins).toMatch(
      /html\[data-theme-editor-shell\]\[data-wallpaper="1"\] \.theme-editor-shell\s*\{[^}]*background:\s*var\(--bg-main\)\s*!important/s,
    );
    const provider = readFileSync(
      resolve(__dirname, "../providers/ThemeProvider.tsx"),
      "utf8",
    );
    expect(provider).toContain("isThemeEditorDocument()");
    expect(provider).toContain("applyWallpaperFlag(false)");
  });

  it("loads the same locale catalog as the main window", () => {
    expect(app).toContain("readThemeEditorBootLocale");
    expect(app).toContain("settingsGet");
    expect(app).toContain("resolveLocalePreference");
    const hook = readFileSync(
      resolve(__dirname, "../hooks/useAppearanceEditorModel.ts"),
      "utf8",
    );
    expect(hook).toContain("loadLocaleCatalog");
    expect(hook).toContain("catalogRev");
  });
});
