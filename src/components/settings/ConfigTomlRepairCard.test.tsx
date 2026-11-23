/** @vitest-environment jsdom */
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/test/jsdomStubs";
import type { Vars } from "@/i18n";
import type { ConfigTomlDiagnosis, ConfigTomlRepairResult } from "@/lib/api";
import { ConfigTomlRepairCard } from "./ConfigTomlRepairCard";

const { configTomlDiagnose, configTomlRepair } = vi.hoisted(() => ({
  configTomlDiagnose: vi.fn(),
  configTomlRepair: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  isTauri: () => true,
  configTomlDiagnose,
  configTomlRepair,
}));

const copy: Record<string, string> = {
  "settings.configTomlRepair": "Repair config.toml",
  "settings.configTomlRepairDesc": "Check the file.",
  "settings.configTomlRepairAction": "Repair config.toml",
  "settings.configTomlRepairChecking": "Checking…",
  "settings.configTomlRepairRunning": "Repairing…",
  "settings.configTomlStatusOk": "This file parses. Nothing to repair.",
  "settings.configTomlStatusMissing": "No config.toml yet.",
  "settings.configTomlStatusEmpty": "The file is empty.",
  "settings.configTomlStatusDuplicate":
    "{n} duplicate keys (for example {examples}).",
  "settings.configTomlStatusSyntax": "This file does not parse as TOML.",
  "settings.configTomlStatusUnreadable": "This file could not be read.",
  "settings.configTomlModeShared": "Shared CLI home",
  "settings.configTomlModeIndependent": "App agent home",
  "settings.configTomlRepairedDeduped":
    "Removed {n} duplicate keys. Backup: {backup}",
  "settings.configTomlRepairedReset":
    "Replaced the file with an empty valid config. Backup: {backup}",
  "settings.configTomlRepairNoop": "Nothing to repair.",
  "settings.configTomlRepairFailed": "Repair failed: {error}",
  "settings.configTomlConfirmTitle": "Repair config.toml?",
  "settings.configTomlConfirmDeduped": "Duplicate keys will be removed.",
  "settings.configTomlConfirmReset": "This file does not parse.",
  "settings.configTomlConfirmAction": "Repair",
  "settings.configTomlNeedDesktop": "This check runs in the desktop app.",
  "settings.configTomlEdit.path": "Config: {path}",
  "common.close": "Close",
  "common.cancel": "Cancel",
};

const t = (key: string, vars?: Vars) => {
  let text = copy[key] ?? key;
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.replaceAll(`{${name}}`, String(value ?? ""));
    }
  }
  return text;
};

const duplicate: ConfigTomlDiagnosis = {
  path: "C:\\Users\\me\\.atlas\\config.toml",
  mode: "shared",
  status: "duplicate",
  duplicateCount: 2,
  examples: ["yolo"],
  detail: "duplicate_keys",
  repairable: true,
};

const healed: ConfigTomlRepairResult = {
  changed: true,
  path: duplicate.path,
  mode: "shared",
  action: "deduped",
  removedDuplicates: 2,
  backupPath: "C:\\Users\\me\\.atlas\\config.toml.bak-heal-1",
  status: "ok",
  detail: "",
};

const okAfter: ConfigTomlDiagnosis = {
  ...duplicate,
  status: "ok",
  duplicateCount: 0,
  examples: [],
  detail: "",
  repairable: false,
};

afterEach(() => {
  cleanup();
  configTomlDiagnose.mockReset();
  configTomlRepair.mockReset();
});

describe("ConfigTomlRepairCard", () => {
  it("diagnoses duplicate keys and repairs after confirm", async () => {
    configTomlDiagnose
      .mockResolvedValueOnce(duplicate)
      .mockResolvedValueOnce(okAfter);
    configTomlRepair.mockResolvedValue(healed);
    const toast = vi.fn();

    render(
      <ConfigTomlRepairCard mode="shared" showSettingsToast={toast} t={t} />,
    );

    expect(await screen.findByText("2 duplicate keys (for example yolo).")).toBeTruthy();
    expect(screen.getByText(/Shared CLI home/)).toBeTruthy();
    const open = screen.getByRole("button", { name: "Repair config.toml" });
    expect(open.hasAttribute("disabled")).toBe(false);
    fireEvent.click(open);

    expect(await screen.findByText("Duplicate keys will be removed.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Repair", exact: true }));

    await waitFor(() => {
      expect(configTomlRepair).toHaveBeenCalledWith("shared");
    });
    expect(
      await screen.findByText(
        "Removed 2 duplicate keys. Backup: C:\\Users\\me\\.atlas\\config.toml.bak-heal-1",
      ),
    ).toBeTruthy();
    expect(await screen.findByText("This file parses. Nothing to repair.")).toBeTruthy();
    expect(toast).toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: "Repair config.toml" }).hasAttribute("disabled"),
    ).toBe(true);
  });

  it("keeps repair disabled when the file already parses", async () => {
    configTomlDiagnose.mockResolvedValue(okAfter);
    render(<ConfigTomlRepairCard mode="independent" t={t} />);
    expect(await screen.findByText("This file parses. Nothing to repair.")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Repair config.toml" }).hasAttribute("disabled"),
    ).toBe(true);
    expect(configTomlRepair).not.toHaveBeenCalled();
  });
});
