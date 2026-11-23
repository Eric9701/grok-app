/** @vitest-environment jsdom */
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/test/jsdomStubs";
import type { Vars } from "@/i18n";
import { EnvCommitPushDialog } from "./EnvCommitPushDialog";

const { gitStatus, gitCommitWorktree, gitPushBranch } = vi.hoisted(() => ({
  gitStatus: vi.fn(),
  gitCommitWorktree: vi.fn(),
  gitPushBranch: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  isTauri: () => true,
  gitStatus,
  gitCommitWorktree,
  gitPushBranch,
}));

const copy: Record<string, string> = {
  "side.env.commitPushTitle": "Commit and push",
  "side.env.commitPushHint": "Commits every change, then pushes.",
  "side.env.commitPushBranch": "Branch: {branch}",
  "side.env.commitPushFiles": "{n} files will be committed",
  "side.env.commitPushMore": "and {n} more",
  "side.env.commitPushClean": "Nothing to commit.",
  "side.env.commitPushMessage": "Commit message",
  "side.env.commitPushMessagePlaceholder": "What changed",
  "side.env.commitPushAction": "Commit and push",
  "side.env.commitPushOnly": "Push branch",
  "side.env.commitPushCommitting": "Committing…",
  "side.env.commitPushPushing": "Pushing…",
  "side.env.commitPushChecking": "Checking changes…",
  "side.env.commitPushDone": "Committed and pushed {branch}",
  "side.env.commitPushPushed": "Pushed {branch}",
  "side.env.commitPushUpToDate": "{branch} is already on origin",
  "side.env.commitPushFailed": "Could not finish: {error}",
  "side.env.commitPushCommittedPushFailed": "Commit saved, push failed: {error}",
  "side.env.commitPushDetached": "Not on a branch.",
  "side.env.commitPushNeedMessage": "Enter a commit message.",
  "side.env.commitPushInvalidMessage": "Bad message.",
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

vi.mock("@/i18n", () => ({
  createT: () => t,
}));

afterEach(() => {
  cleanup();
  gitStatus.mockReset();
  gitCommitWorktree.mockReset();
  gitPushBranch.mockReset();
});

describe("EnvCommitPushDialog", () => {
  it("commits dirty files and then pushes", async () => {
    gitStatus.mockResolvedValue({
      available: true,
      branch: "dev",
      files: [
        { path: "AGENTS.md", name: "AGENTS.md", status: " M" },
        { path: "CHANGELOG.md", name: "CHANGELOG.md", status: " M" },
      ],
    });
    gitCommitWorktree.mockResolvedValue({
      available: true,
      ok: true,
      committed: true,
      branch: "dev",
    });
    gitPushBranch.mockResolvedValue({
      available: true,
      ok: true,
      branch: "dev",
      stdout: "dev -> dev",
      stderr: "",
    });
    const onClose = vi.fn();
    const onToast = vi.fn();
    const onDone = vi.fn();

    render(
      <EnvCommitPushDialog
        open
        locale="en"
        projectPath="E:/repo"
        onClose={onClose}
        onToast={onToast}
        onDone={onDone}
      />,
    );

    expect(await screen.findByText("2 files will be committed")).toBeTruthy();
    const submit = screen.getByTestId("env-commit-push-submit");
    expect(submit.hasAttribute("disabled")).toBe(true);
    fireEvent.change(screen.getByTestId("env-commit-message"), {
      target: { value: "docs: note the change" },
    });
    expect(submit.hasAttribute("disabled")).toBe(false);
    fireEvent.click(submit);

    await waitFor(() => {
      expect(gitCommitWorktree).toHaveBeenCalledWith(
        "E:/repo",
        "docs: note the change",
      );
    });
    await waitFor(() => {
      expect(gitPushBranch).toHaveBeenCalledWith("E:/repo");
    });
    expect(onToast).toHaveBeenCalledWith("Committed and pushed dev", 4200);
    expect(onDone).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("pushes a clean branch without committing", async () => {
    gitStatus.mockResolvedValue({
      available: true,
      branch: "dev",
      files: [],
    });
    gitPushBranch.mockResolvedValue({
      available: true,
      ok: true,
      branch: "dev",
      stdout: "",
      stderr: "Everything up-to-date",
    });
    const onToast = vi.fn();

    render(
      <EnvCommitPushDialog
        open
        locale="en"
        projectPath="E:/repo"
        onClose={() => {}}
        onToast={onToast}
      />,
    );

    expect(await screen.findByText("Nothing to commit.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Push branch" }));
    await waitFor(() => {
      expect(gitPushBranch).toHaveBeenCalledWith("E:/repo");
    });
    expect(gitCommitWorktree).not.toHaveBeenCalled();
    expect(onToast).toHaveBeenCalledWith("dev is already on origin", 4200);
  });
});
