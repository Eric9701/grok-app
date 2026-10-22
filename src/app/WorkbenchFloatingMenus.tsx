/**
 * Floating project/session + composer context menus.
 */
import { useEffect, useState, type Dispatch, type SetStateAction } from "react";
import { ContextMenu, type ContextMenuItem } from "@/components/ContextMenu";
import * as api from "@/lib/api";
import { AGENT_RUNTIMES_EVENT } from "@/lib/api/agentRuntimes";
import { buildProjectContextMenuItems, type WorkbenchProjectContextMenuProps } from "@/app/WorkbenchProjectContextMenu";
import { buildSessionContextMenuItems, type WorkbenchSessionContextMenuProps } from "@/app/WorkbenchSessionContextMenu";

export type WorkbenchFloatingMenusProps =
  WorkbenchProjectContextMenuProps &
    WorkbenchSessionContextMenuProps & {
      composerCtxItems: ContextMenuItem[];
      composerCtxMenu: { x: number; y: number } | null;
      setComposerCtxMenu: Dispatch<
        SetStateAction<{ x: number; y: number } | null>
      >;
    };

function useNonAtlasSessionIds(): ReadonlySet<string> {
  const [ids, setIds] = useState<ReadonlySet<string>>(() => new Set());
  useEffect(() => {
    if (!api.isDesktopHost()) return;
    let stop = false;
    const load = () => {
      void api.agentRuntimesGet().then((file) => {
        if (stop) return;
        const kinds = new Map(file.profiles.map((profile) => [profile.id, profile.kind]));
        const next = new Set<string>();
        for (const binding of file.sessionBindings) {
          const kind = kinds.get(binding.profileId);
          if (kind && kind !== "atlas") next.add(binding.sessionId);
        }
        setIds(next);
      });
    };
    load();
    window.addEventListener(AGENT_RUNTIMES_EVENT, load);
    return () => {
      stop = true;
      window.removeEventListener(AGENT_RUNTIMES_EVENT, load);
    };
  }, []);
  return ids;
}

export function WorkbenchFloatingMenus(p: WorkbenchFloatingMenusProps) {
  const {
    composerCtxItems,
    composerCtxMenu,
    ctxMenu,
    setComposerCtxMenu,
    setCtxMenu,
  } = p;
  const nonAtlasSessionIds = useNonAtlasSessionIds();
  const kind = ctxMenu?.kind;
  const items =
    kind === "session" || kind === "session-move"
      ? buildSessionContextMenuItems({ ...p, nonAtlasSessionIds })
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
