/**
 * Floating project/session + composer context menus.
 */
import { ContextMenu } from "@/components/ContextMenu";
import { buildProjectContextMenuItems } from "@/app/WorkbenchProjectContextMenu";
import { buildSessionContextMenuItems } from "@/app/WorkbenchSessionContextMenu";

export type WorkbenchFloatingMenusProps = {
  [key: string]: any;
};

export function WorkbenchFloatingMenus(p: WorkbenchFloatingMenusProps) {
  const {
    composerCtxItems,
    composerCtxMenu,
    ctxMenu,
    setComposerCtxMenu,
    setCtxMenu,
  } = p;
  const kind = ctxMenu?.kind;
  const items =
    kind === "session" || kind === "session-move"
      ? buildSessionContextMenuItems(p)
      : buildProjectContextMenuItems(p);
  return (
          <>
          <ContextMenu
            open={!!ctxMenu && items.length > 0}
            x={ctxMenu?.x ?? 0}
            y={ctxMenu?.y ?? 0}
            onClose={() => setCtxMenu(null)}
            items={items}
            estimatedHeight={
              ctxMenu?.kind === "session"
                ? 360
                : ctxMenu?.kind === "project-policy"
                  ? 280
                  : 240
            }
          />
          <ContextMenu
            open={!!composerCtxMenu}
            x={composerCtxMenu?.x ?? 0}
            y={composerCtxMenu?.y ?? 0}
            onClose={() => setComposerCtxMenu(null)}
            items={composerCtxItems}
            estimatedHeight={88}
          />
          </>
  );
}
