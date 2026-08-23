/**
 * App-chrome overlays: doctor, project rules, prompt-history / archive
 * confirms, worktrees, shortcuts, tutorial, live voice.
 *
 * Compact / queue / ask-user / rewind stay with composer or session-turn
 * owners. Host still owns openSession / send / dialog verbs.
 */
import { lazy, Suspense } from "react";
import { ArchiveAgeConfirmModal } from "@/components/workbench-modals/ArchiveAgeConfirmModal";
import { PromptHistoryClearModal } from "@/components/workbench-modals/PromptHistoryClearModal";
import { ShortcutsHelpModal } from "@/components/workbench-modals/ShortcutsHelpModal";
import { WorktreeCreateModal } from "@/components/workbench-modals/WorktreeCreateModal";
import { WorktreeGcModal } from "@/components/workbench-modals/WorktreeGcModal";
import {
  WorktreeShipModal,
  type WorktreeShipSuccess,
} from "@/components/workbench-modals/WorktreeShipModal";
import type { Locale } from "@/i18n";
import type { GitWorktreeGcResult } from "@/lib/api";
import type { AppPlatform } from "@/lib/appPlatform";
import type { ComposerSendKeyPref } from "@/lib/composerSendKey";
import type { WorktreeLayout } from "@/lib/gitWorktree";
import type { ArchiveAgePlan } from "@/lib/sessionArchiveAge";
import type { ShortcutRemapMap } from "@/lib/shortcutRemap";
import type { VoiceSessionChipInput } from "@/lib/voiceCommandCenter";

const DoctorModal = lazy(async () => {
  const m = await import("@/components/DoctorModal");
  return { default: m.DoctorModal };
});
const ProjectRulesModal = lazy(async () => {
  const m = await import("@/components/ProjectRulesModal");
  return { default: m.ProjectRulesModal };
});
const ProductTutorial = lazy(async () => {
  const m = await import("@/components/ProductTutorial");
  return { default: m.ProductTutorial };
});
const VoiceOverlay = lazy(async () => {
  const m = await import("@/components/VoiceOverlay");
  return { default: m.VoiceOverlay };
});

export type WorkbenchChromeOverlaysProps = {
  locale: Locale;
  platform: AppPlatform;
  showDoctor: boolean;
  closeDoctor: () => void;
  onDoctorConfirm: (opts: {
    title: string;
    message: string;
    confirmLabel?: string;
    danger?: boolean;
    onConfirm: () => void;
  }) => void;
  onDoctorResetDone: () => void;
  onOpenReliability: () => void;
  projectRulesTarget: { path: string; name: string } | null;
  closeProjectRules: () => void;
  promptHistoryClearOpen: boolean;
  closePromptHistoryClear: () => void;
  onConfirmPromptHistoryClear: () => void;
  archiveAgePlan: ArchiveAgePlan | null;
  archiveAgeBusy: boolean;
  closeArchiveAge: () => void;
  onConfirmArchiveAge: () => void;
  worktreeCreateOpen: boolean;
  worktreeCreateBusy: boolean;
  worktreeCreateStartChat: boolean;
  worktreeCreateName: string;
  worktreeCreateLayout: WorktreeLayout;
  worktreeCreateRef: string;
  worktreeCreatePreviewPath: string | null;
  worktreeCreateError: string | null;
  closeWorktreeCreate: () => void;
  submitWorktreeCreate: () => void;
  onWorktreeCreateNameChange: (value: string) => void;
  onWorktreeCreateLayoutChange: (value: WorktreeLayout) => void;
  onWorktreeCreateRefChange: (value: string) => void;
  worktreeGcOpen: boolean;
  worktreeGcBusy: boolean;
  worktreeGcPreviewBusy: boolean;
  worktreeGcForce: boolean;
  worktreeGcPreview: GitWorktreeGcResult | null;
  worktreeGcError: string | null;
  closeWorktreeGc: () => void;
  submitWorktreeGc: () => void;
  setWorktreeGcForce: (value: boolean) => void;
  shipOpen: boolean;
  shipBusy: boolean;
  shipSuccess: WorktreeShipSuccess | null;
  shipTitle: string;
  shipBody: string;
  shipCreatePr: boolean;
  shipDraft: boolean;
  shipBranch: string | null;
  shipStatus: string | null;
  shipError: string | null;
  closeShip: () => void;
  submitShip: () => void;
  onShipTitleChange: (value: string) => void;
  onShipBodyChange: (value: string) => void;
  setShipCreatePr: (value: boolean) => void;
  setShipDraft: (value: boolean) => void;
  onOpenPrHubFromShip: (prNumber: number | null) => void;
  showToast: (message: string, ms: number) => void;
  showShortcuts: boolean;
  composerSendKeyPref: ComposerSendKeyPref;
  shortcutRemaps: ShortcutRemapMap;
  voiceHotkeyEnabled: boolean;
  closeShortcuts: () => void;
  showProductTutorial: boolean;
  closeProductTutorial: () => void;
  liveVoiceOpen: boolean;
  voiceLocale: Locale;
  voiceProjectPath: string | null | undefined;
  voiceProjectId: string | null;
  voiceProjectName: string;
  voiceId: string | null | undefined;
  voiceKeepAgentsOnEnd: boolean;
  voiceHasActiveSession: boolean;
  voiceHasAuth: boolean;
  voiceSessions: VoiceSessionChipInput[];
  closeLiveVoice: () => void;
  onLiveVoiceClassifiedNotice: (message: string) => void;
  onSendVoiceTranscriptAsPrompt:
    | ((prompt: string) => Promise<void>)
    | undefined;
  onVoiceFocusSession: (id: string) => void;
};

export function WorkbenchChromeOverlays(p: WorkbenchChromeOverlaysProps) {
  return (
    <>
      {p.showDoctor ? (
        <Suspense fallback={null}>
          <DoctorModal
            open={p.showDoctor}
            onClose={p.closeDoctor}
            locale={p.locale}
            onConfirm={p.onDoctorConfirm}
            onResetDone={p.onDoctorResetDone}
            onOpenReliability={p.onOpenReliability}
          />
        </Suspense>
      ) : null}
      {p.projectRulesTarget ? (
        <Suspense fallback={null}>
          <ProjectRulesModal
            open
            onClose={p.closeProjectRules}
            projectPath={p.projectRulesTarget.path}
            projectName={p.projectRulesTarget.name}
            locale={p.locale}
          />
        </Suspense>
      ) : null}
      <PromptHistoryClearModal
        locale={p.locale}
        open={p.promptHistoryClearOpen}
        onClose={p.closePromptHistoryClear}
        onConfirm={p.onConfirmPromptHistoryClear}
      />
      <ArchiveAgeConfirmModal
        locale={p.locale}
        plan={p.archiveAgePlan}
        busy={p.archiveAgeBusy}
        onClose={p.closeArchiveAge}
        onConfirm={p.onConfirmArchiveAge}
      />
      <WorktreeCreateModal
        locale={p.locale}
        open={p.worktreeCreateOpen}
        busy={p.worktreeCreateBusy}
        startChat={p.worktreeCreateStartChat}
        name={p.worktreeCreateName}
        layout={p.worktreeCreateLayout}
        startRef={p.worktreeCreateRef}
        previewPath={p.worktreeCreatePreviewPath}
        error={p.worktreeCreateError}
        onClose={p.closeWorktreeCreate}
        onSubmit={p.submitWorktreeCreate}
        onNameChange={p.onWorktreeCreateNameChange}
        onLayoutChange={p.onWorktreeCreateLayoutChange}
        onRefChange={p.onWorktreeCreateRefChange}
      />
      <WorktreeGcModal
        locale={p.locale}
        open={p.worktreeGcOpen}
        busy={p.worktreeGcBusy}
        previewBusy={p.worktreeGcPreviewBusy}
        force={p.worktreeGcForce}
        preview={p.worktreeGcPreview}
        error={p.worktreeGcError}
        onClose={p.closeWorktreeGc}
        onSubmit={p.submitWorktreeGc}
        onForceChange={p.setWorktreeGcForce}
      />
      <WorktreeShipModal
        locale={p.locale}
        open={p.shipOpen}
        busy={p.shipBusy}
        success={p.shipSuccess}
        title={p.shipTitle}
        body={p.shipBody}
        createPr={p.shipCreatePr}
        draft={p.shipDraft}
        branch={p.shipBranch}
        status={p.shipStatus}
        error={p.shipError}
        onClose={p.closeShip}
        onSubmit={p.submitShip}
        onTitleChange={p.onShipTitleChange}
        onBodyChange={p.onShipBodyChange}
        onCreatePrChange={p.setShipCreatePr}
        onDraftChange={p.setShipDraft}
        onOpenPrHub={p.onOpenPrHubFromShip}
        onToast={p.showToast}
      />
      <ShortcutsHelpModal
        locale={p.locale}
        open={p.showShortcuts}
        platform={p.platform}
        composerSendKeyPref={p.composerSendKeyPref}
        shortcutRemaps={p.shortcutRemaps}
        voiceHotkeyEnabled={p.voiceHotkeyEnabled}
        onClose={p.closeShortcuts}
      />
      {p.showProductTutorial ? (
        <Suspense fallback={null}>
          <ProductTutorial
            open={p.showProductTutorial}
            locale={p.locale}
            onClose={p.closeProductTutorial}
            onSkip={p.closeProductTutorial}
            onDone={p.closeProductTutorial}
          />
        </Suspense>
      ) : null}
      {p.liveVoiceOpen ? (
        <Suspense fallback={null}>
          <VoiceOverlay
            locale={p.voiceLocale}
            open={p.liveVoiceOpen}
            projectPath={p.voiceProjectPath}
            projectId={p.voiceProjectId}
            projectName={p.voiceProjectName}
            voiceId={p.voiceId}
            keepAgentsOnEnd={p.voiceKeepAgentsOnEnd}
            hasActiveSession={p.voiceHasActiveSession}
            hasVoiceAuth={p.voiceHasAuth}
            sessions={p.voiceSessions}
            onClose={p.closeLiveVoice}
            onClassifiedNotice={p.onLiveVoiceClassifiedNotice}
            onSendTranscriptAsPrompt={p.onSendVoiceTranscriptAsPrompt}
            onFocusSession={p.onVoiceFocusSession}
          />
        </Suspense>
      ) : null}
    </>
  );
}
