/**
 * Codex-style task chips stacked above the living mark.
 * Each chip is one session: in-progress or completed. Click opens that chat.
 */
import type { Ref } from "react";
import { IconAlertTriangle, IconCheck } from "@/components/icons";
import { createT } from "@/i18n";
import type { PetTask } from "@/lib/pet";
import { PET_BUBBLE_WIDTH, petBubbleViewportHeight } from "@/lib/pet";

export function PetTaskBubbles({
  tasks,
  t,
  onOpen,
  listRef,
}: {
  tasks: readonly PetTask[];
  t: ReturnType<typeof createT>;
  onOpen: (sessionId: string) => void;
  listRef?: Ref<HTMLDivElement>;
}) {
  if (tasks.length === 0) return null;
  return (
    <div
      ref={listRef}
      className="pet-bubbles"
      role="list"
      aria-label={t("pet.bubble.list")}
      style={{ width: PET_BUBBLE_WIDTH, maxHeight: petBubbleViewportHeight() }}
      onWheel={(e) => e.stopPropagation()}
    >
      {tasks.map((task) => {
        const title = task.title?.trim() || t("pet.bubble.untitled");
        const phaseLabel =
          task.phase === "done"
            ? t("pet.bubble.progressDone")
            : t("pet.bubble.progressActive");
        const pct = Math.round(Math.max(0, Math.min(1, task.progress)) * 100);
        return (
          <button
            key={task.sessionId}
            type="button"
            role="listitem"
            className={
              "pet-bubble" +
              (task.phase === "done" ? " is-done" : " is-active") +
              (task.kind === "error" ? " is-error" : "") +
              (task.kind === "needs_you" ? " is-wait" : "")
            }
            aria-label={t("pet.bubble.open", { title })}
            title={`${phaseLabel} · ${title}`}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onOpen(task.sessionId);
            }}
          >
            <span className="pet-bubble__row">
              <span className="pet-bubble__glyph" aria-hidden>
                {task.kind === "error" ? (
                  <IconAlertTriangle size={13} stroke={2.2} />
                ) : task.phase === "done" ? (
                  <IconCheck size={13} stroke={2.4} />
                ) : (
                  <span className="pet-bubble__spin" />
                )}
              </span>
              <span className="pet-bubble__text">
                <span className="pet-bubble__title">{title}</span>
                {task.toolTitle ? (
                  <span className="pet-bubble__sub">{task.toolTitle}</span>
                ) : (
                  <span className="pet-bubble__sub">{phaseLabel}</span>
                )}
              </span>
            </span>
            <span className="pet-bubble__track" aria-hidden>
              <span
                className="pet-bubble__fill"
                style={
                  task.phase === "active"
                    ? undefined
                    : { width: `${pct}%` }
                }
              />
            </span>
          </button>
        );
      })}
    </div>
  );
}
