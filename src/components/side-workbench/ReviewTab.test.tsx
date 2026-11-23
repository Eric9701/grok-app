/**
 * @vitest-environment jsdom
 */
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { REVIEW_TREE_WIDTH_KEY } from "@/lib/reviewTreeLayout";
import { ReviewTab } from "./ReviewTab";

const LONG_NAME = "excel-export-style-2026-11-19.md";

afterEach(cleanup);

beforeEach(() => {
  localStorage.clear();
});

function renderTree() {
  render(
    <ReviewTab
      locale="zh"
      projectPath={null}
      sessionChanges={[
        {
          path: `src/cms-special-allowance-backend/${LONG_NAME}`,
          name: LONG_NAME,
          toolKind: "write",
          status: "completed",
          updatedAt: "2026-11-19T00:00:00.000Z",
        },
      ]}
    />,
  );
}

function stubSplitBox(width: number) {
  const split = document.querySelector(".sw-review__split") as HTMLElement;
  split.getBoundingClientRect = () =>
    ({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: width,
      bottom: 600,
      width,
      height: 600,
      toJSON() {
        return {};
      },
    }) as DOMRect;
}

function treeName(text: string): HTMLElement | undefined {
  const tree = screen.getByTestId("review-tree");
  return [...tree.querySelectorAll(".sw-review-tree__name")].find(
    (el) => el.textContent === text,
  ) as HTMLElement | undefined;
}

describe("ReviewTab change tree", () => {
  it("shows the full node text and a drag handle at the default width", () => {
    renderTree();
    expect(treeName(LONG_NAME)?.textContent).toBe(LONG_NAME);
    expect(treeName("cms-special-allowance-backend")?.textContent).toBe(
      "cms-special-allowance-backend",
    );
    expect(screen.getByTestId("review-tree-resizer").getAttribute("aria-label")).toBe(
      "调整文件树宽度",
    );
    expect((screen.getByTestId("review-tree") as HTMLElement).style.width).toBe(
      "240px",
    );
  });

  it("drags the column wider and keeps that width", () => {
    renderTree();
    stubSplitBox(1000);
    act(() => {
      fireEvent.pointerDown(screen.getByTestId("review-tree-resizer"));
    });
    act(() => {
      window.dispatchEvent(new PointerEvent("pointermove", { clientX: 640 }));
    });
    expect((screen.getByTestId("review-tree") as HTMLElement).style.width).toBe(
      "360px",
    );
    act(() => {
      window.dispatchEvent(new PointerEvent("pointerup"));
    });
    expect(localStorage.getItem(REVIEW_TREE_WIDTH_KEY)).toBe("360");
    expect(treeName(LONG_NAME)?.textContent).toBe(LONG_NAME);
  });
});
