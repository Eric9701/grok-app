/**
 * @vitest-environment jsdom
 *
 * Palette open/query/filters live in this hook. AppWorkbench only supplies
 * action dispatch and session/project pick.
 */
import { describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { createT } from "@/i18n";
import { useSearchPalette } from "./useSearchPalette";
import type { PaletteActionDef } from "@/lib/paletteActions";

function setup(onRunAction = vi.fn()) {
  return renderHook(() =>
    useSearchPalette({
      sessions: [
        { id: "s1", title: "Hello world", projectId: null },
      ],
      projects: [{ id: "p1", name: "grok-app", path: "/code/grok-app" }],
      tr: createT("en"),
      onRunAction,
      onPickProject: vi.fn(),
      onPickSession: vi.fn(),
    }),
  );
}

describe("useSearchPalette", () => {
  it("openBlank clears the query and opens", () => {
    const { result } = setup();
    act(() => {
      result.current.setQuery("old");
    });
    act(() => {
      result.current.openBlank();
    });
    expect(result.current.open).toBe(true);
    expect(result.current.query).toBe("");
  });

  it("closes and clears query before dispatching an action", () => {
    const onRunAction = vi.fn();
    const { result } = setup(onRunAction);
    act(() => {
      result.current.openBlank();
    });
    const action: PaletteActionDef = result.current.actions[0];
    expect(action).toBeTruthy();
    act(() => {
      result.current.runAction(action);
    });
    expect(result.current.open).toBe(false);
    expect(result.current.query).toBe("");
    expect(onRunAction).toHaveBeenCalledWith(action);
  });

  it("filters sessions by query without asking the host", () => {
    const { result } = setup();
    act(() => {
      result.current.openBlank();
      result.current.setQuery("hello");
    });
    expect(result.current.sessionHits.map((h) => h.id)).toEqual(["s1"]);
    act(() => {
      result.current.setQuery("zzzz");
    });
    expect(result.current.sessionHits).toEqual([]);
  });
});
