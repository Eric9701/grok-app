import { lazy, Suspense, type ReactNode } from "react";
import { FleetPage } from "@/fleet/FleetPage";
import type { AutomationsPageProps } from "@/components/AutomationsPage";
import type { KanbanBoardPageProps } from "@/components/KanbanBoardPage";
import { aiCreateSeedPrompt, type Automation } from "@/lib/automations";
import * as api from "@/lib/api";
import type { ModelOption } from "@/lib/grokCatalog";
import type { WorkbenchHashPane } from "@/lib/workbenchHash";
import type { Locale, MessageKey } from "@/i18n";

const AutomationsPage = lazy(async () => {
  const m = await import("@/components/AutomationsPage");
  return { default: m.AutomationsPage };
});
const KanbanBoardPage = lazy(async () => {
  const m = await import("@/components/KanbanBoardPage");
  return { default: m.KanbanBoardPage };
});

type TFn = (key: MessageKey, vars?: Record<string, string | number>) => string;

export function WorkbenchMainPane({
  mainPane,
  locale,
  sessions,
  projects,
  liveMap,
  currentSessionId,
  generalWorkspacePath,
  onSelectSession,
  tr,
  modelId,
  effort,
  models,
  launchAtLogin,
  closeToTray,
  keepTrayForSchedules,
  onKeepTrayForSchedules,
  onNavigateSettings,
  onNewChat,
  onToast,
  onRunNow,
  children,
}: {
  mainPane: WorkbenchHashPane;
  locale: Locale;
  sessions: KanbanBoardPageProps["sessions"];
  projects: Array<{ id: string; name: string; path: string }>;
  liveMap: KanbanBoardPageProps["liveMap"];
  currentSessionId?: string | null;
  generalWorkspacePath?: string | null;
  onSelectSession: (sessionId: string) => void;
  tr: TFn;
  modelId: string;
  effort: string;
  models: ModelOption[];
  launchAtLogin: boolean;
  closeToTray: boolean;
  keepTrayForSchedules: boolean;
  onKeepTrayForSchedules: (value: boolean) => void;
  onNavigateSettings: (section?: "general" | null, tab?: string | null) => void;
  onNewChat: (
    project?: null,
    opts?: {
      seedDraft?: string;
      switchToChat?: boolean;
      automationSetup?: boolean;
    },
  ) => void | Promise<void>;
  onToast: (message: string | null) => void;
  onRunNow: (auto: Automation) => void;
  children: ReactNode;
}) {
  if (mainPane === "fleet") {
    return <FleetPage locale={locale} />;
  }
  if (mainPane === "kanban") {
    return (
      <Suspense fallback={null}>
        <KanbanBoardPage
          locale={locale}
          sessions={sessions}
          projects={projects.map((project) => ({
            id: project.id,
            name: project.name,
            path: project.path,
          }))}
          liveMap={liveMap}
          currentSessionId={currentSessionId}
          untitledLabel={tr("session.untitled")}
          generalWorkspacePath={generalWorkspacePath}
          unboundProjectLabel={tr("sidebar.otherSessions")}
          onSelectSession={onSelectSession}
        />
      </Suspense>
    );
  }
  if (mainPane === "automations") {
    const t: AutomationsPageProps["t"] = (key, vars) =>
      tr(key as MessageKey, vars);
    const jump = (anchorId: string) => {
      onNavigateSettings("general", "app");
      window.setTimeout(() => {
        const el = document.getElementById(anchorId);
        if (!el) return;
        el.scrollIntoView({ block: "center", behavior: "smooth" });
        el.classList.add("is-search-hit");
        window.setTimeout(() => el.classList.remove("is-search-hit"), 1600);
      }, 120);
    };
    return (
      <Suspense fallback={null}>
        <AutomationsPage
          t={t}
          projects={projects.map((project) => ({
            id: project.id,
            name: project.name,
          }))}
          defaultModelId={modelId}
          defaultEffort={effort}
          models={models}
          openAtLogin={launchAtLogin}
          onOpenLaunchAtLogin={() => jump("settings-anchor-launchAtLogin")}
          closeToTray={closeToTray}
          keepTrayForSchedules={keepTrayForSchedules}
          onKeepTrayForSchedules={(value) => {
            onKeepTrayForSchedules(value);
            void api.settingsGet().then((settings) =>
              api.settingsSet({ ...settings, keepTrayForSchedules: value }),
            );
          }}
          onOpenKeepTraySetting={() => jump("settings-anchor-keepTrayForSchedules")}
          onAiCreate={() => {
            void onNewChat(null, {
              seedDraft: aiCreateSeedPrompt("Atlas"),
              switchToChat: true,
              automationSetup: true,
            });
            onToast(tr("automations.aiComposerHint"));
            window.setTimeout(() => onToast(null), 4200);
          }}
          onRunNow={(auto) => onRunNow(auto)}
        />
      </Suspense>
    );
  }
  return <>{children}</>;
}
