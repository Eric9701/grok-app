/** @vitest-environment jsdom */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/test/jsdomStubs";
import { createT } from "@/i18n";
import type { FleetTask } from "@/lib/api/fleet";
import { FleetBoard } from "./FleetBoard";
import { preferredAgent } from "./pickAgent";

afterEach(() => {
  cleanup();
});

const tr = createT("zh");

describe("FleetBoard", () => {
  it("shows the task preview and hides step text until a trace is open", () => {
    render(
      <FleetBoard
        tr={(key) => tr(key)}
        status={{
          enabled: true,
          savedBind: "127.0.0.1:24720",
          listening: true,
          tokenLast4: "abcd",
          machines: [{ agentId: "box", online: true, local: false }],
        }}
        tasks={[
          {
            id: "t1",
            agentId: "box",
            cwd: "/data/proj",
            preview: "ship the note",
            status: "running",
            createdAtMs: 1,
            sessionId: "",
            unreachable: false,
          },
        ]}
        trace={null}
        agentId="box"
        cwd="/data/proj"
        prompt=""
        busy={false}
        onAgentId={() => {}}
        onCwd={() => {}}
        onPrompt={() => {}}
        onDispatch={() => {}}
        onCancel={() => {}}
        onOpen={() => {}}
        onCloseTrace={() => {}}
      />,
    );
    expect(screen.getByText("ship the note")).toBeTruthy();
    expect(screen.queryByText("secret-step-body")).toBeNull();
    expect(screen.getByText(tr("fleet.cwdHint"))).toBeTruthy();
  });

  it("shows bind failure and an empty remote roster", () => {
    render(
      <FleetBoard
        tr={(key) => tr(key)}
        status={{
          enabled: true,
          savedBind: "10.0.0.8:24720",
          listening: false,
          listenError: "address in use",
          tokenLast4: "",
          machines: [{ agentId: "local", online: true, local: true }],
        }}
        tasks={[]}
        trace={null}
        agentId="local"
        cwd=""
        prompt=""
        busy={false}
        onAgentId={() => {}}
        onCwd={() => {}}
        onPrompt={() => {}}
        onDispatch={() => {}}
        onCancel={() => {}}
        onOpen={() => {}}
        onCloseTrace={() => {}}
      />,
    );
    expect(screen.getAllByText(tr("fleet.bindError")).length).toBeGreaterThan(0);
    expect(screen.getByText(tr("fleet.machinesEmpty"))).toBeTruthy();
  });

  it("fills the machine name from a roster row", () => {
    const onAgentId = vi.fn();
    render(
      <FleetBoard
        tr={tr}
        status={{
          enabled: true,
          savedBind: "127.0.0.1:24720",
          listening: true,
          tokenLast4: "abcd",
          machines: [
            { agentId: "local", online: true, local: true },
            { agentId: "worker-2", online: true, local: false },
          ],
        }}
        tasks={[]}
        trace={null}
        agentId="local"
        cwd=""
        prompt=""
        busy={false}
        onAgentId={onAgentId}
        onCwd={() => {}}
        onPrompt={() => {}}
        onDispatch={() => {}}
        onCancel={() => {}}
        onOpen={() => {}}
        onCloseTrace={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /worker-2/ }));
    expect(onAgentId).toHaveBeenCalledWith("worker-2");
  });

  it("marks a queued task that is waiting on the same machine", () => {
    render(
      <FleetBoard
        tr={tr}
        status={idleStatus}
        tasks={[
          task({ id: "run", status: "running", preview: "current" }),
          task({ id: "wait", status: "queued", preview: "later" }),
        ]}
        trace={null}
        agentId="worker-2"
        cwd=""
        prompt=""
        busy={false}
        onAgentId={() => {}}
        onCwd={() => {}}
        onPrompt={() => {}}
        onDispatch={() => {}}
        onCancel={() => {}}
        onOpen={() => {}}
        onCloseTrace={() => {}}
      />,
    );
    expect(screen.getByText(tr("fleet.waiting"))).toBeTruthy();
  });

  it("names the original task on an automatic retry", () => {
    render(
      <FleetBoard
        tr={tr}
        status={idleStatus}
        tasks={[
          task({ id: "old", status: "failed", preview: "ship the note", failureReason: "runtime_offline" }),
          task({ id: "retry-1", status: "queued", preview: "again", retryOf: "old" }),
        ]}
        trace={null}
        agentId="worker-2"
        cwd=""
        prompt=""
        busy={false}
        onAgentId={() => {}}
        onCwd={() => {}}
        onPrompt={() => {}}
        onDispatch={() => {}}
        onCancel={() => {}}
        onOpen={() => {}}
        onCloseTrace={() => {}}
      />,
    );
    expect(screen.getByText("重试 ship the note")).toBeTruthy();
    expect(screen.getByText(tr("fleet.reason.offline"))).toBeTruthy();
  });
});

const idleStatus = {
  enabled: true,
  savedBind: "127.0.0.1:24720",
  listening: true,
  tokenLast4: "abcd",
  machines: [{ agentId: "worker-2", online: true, local: false }],
};

function task(partial: Partial<FleetTask> & Pick<FleetTask, "id" | "status" | "preview">): FleetTask {
  return {
    agentId: "worker-2",
    cwd: "/data/proj",
    createdAtMs: 1,
    sessionId: "",
    unreachable: false,
    ...partial,
  };
}

describe("preferredAgent", () => {
  const remote = { agentId: "worker-2", online: true, local: false };

  it("replaces the untouched local default with an online remote", () => {
    expect(preferredAgent([remote], "local", false)).toBe("worker-2");
    expect(preferredAgent([remote], "local", true)).toBeNull();
    expect(preferredAgent([remote], "worker-1", false)).toBeNull();
  });
});
