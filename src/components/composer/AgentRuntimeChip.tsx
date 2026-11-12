/**
 * Composer chip: which CLI this desktop chat runs.
 * Hidden until at least one runtime profile exists.
 * A new chat has no session id yet; the chip still shows, starting from the
 * desktop default, and the pick is applied when that chat is created.
 */
import { useEffect, useState } from "react";
import { GlassModal } from "@/components/GlassModal";
import { Select, type SelectOption } from "@/components/Select";
import * as api from "@/lib/api";
import {
  AGENT_RUNTIMES_EVENT,
  type AgentRuntimesFile,
} from "@/lib/api/agentRuntimes";
import {
  draftRuntimeOverride,
  resolveDraftProfileId,
  setDraftRuntimeOverride,
  subscribeDraftRuntime,
} from "@/lib/draftRuntime";
import type { createT } from "@/i18n";

type TFn = ReturnType<typeof createT>;

export function useAgentRuntimesFile(): AgentRuntimesFile | null {
  const [file, setFile] = useState<AgentRuntimesFile | null>(null);
  useEffect(() => {
    if (!api.isDesktopHost()) return;
    let stop = false;
    const load = () => {
      void api.agentRuntimesGet().then((next) => {
        if (!stop) setFile(next);
      });
    };
    load();
    window.addEventListener(AGENT_RUNTIMES_EVENT, load);
    return () => {
      stop = true;
      window.removeEventListener(AGENT_RUNTIMES_EVENT, load);
    };
  }, []);
  return file;
}

export function useDraftRuntimeOverride(): string | undefined {
  const [picked, setPicked] = useState(draftRuntimeOverride);
  useEffect(() => subscribeDraftRuntime(() => setPicked(draftRuntimeOverride())), []);
  return picked;
}

export function AgentRuntimeChip({
  sessionId,
  projectPath,
  busy,
  tr,
}: {
  sessionId: string | null;
  projectPath?: string | null;
  busy: boolean;
  tr: TFn;
}) {
  const file = useAgentRuntimesFile();
  const draftPick = useDraftRuntimeOverride();
  const [pending, setPending] = useState<string | null>(null);
  const [claudeModel, setClaudeModel] = useState("");

  const current = !file
    ? ""
    : sessionId
      ? file.sessionBindings.find((row) => row.sessionId === sessionId)?.profileId ??
        ""
      : resolveDraftProfileId(draftPick, file.desktopDefaultProfileId);
  const currentProfile = file?.profiles.find((profile) => profile.id === current);
  const claudeFollowsSettings =
    currentProfile?.kind === "claude" && !currentProfile.model.trim();
  const claudeHome = claudeFollowsSettings ? (currentProfile?.home ?? "") : null;

  useEffect(() => {
    if (claudeHome === null || !api.isDesktopHost()) {
      setClaudeModel("");
      return;
    }
    let stop = false;
    void api
      .agentRuntimesClaudeModel(claudeHome, projectPath)
      .then((model) => {
        if (!stop) setClaudeModel(model ?? "");
      })
      .catch(() => {
        if (!stop) setClaudeModel("");
      });
    return () => {
      stop = true;
    };
  }, [claudeHome, projectPath]);

  if (!file || file.profiles.length === 0) return null;

  const foreign = !!currentProfile && currentProfile.kind !== "atlas";
  const shownModel = currentProfile?.model.trim() || claudeModel;
  const options: SelectOption[] = [
    { value: "", label: tr("composer.runtimeLegacy") },
    ...file.profiles
      .filter((profile) => profile.enabled)
      .map((profile) => ({
        value: profile.id,
        label: profile.label || profile.kind,
      })),
  ];

  const commit = (profileId: string) => {
    if (!sessionId) return;
    void api.agentRuntimesSetSessionBinding(sessionId, profileId).then(() =>
      api.sessionConnect({ sessionId }),
    );
  };

  return (
    <>
      <Select
        variant="chip"
        value={current}
        options={options}
        disabled={busy}
        aria-label={tr("composer.runtime")}
        title={busy ? tr("composer.runtimeBusy") : tr("composer.runtime")}
        onChange={(next) => {
          if (next === current) return;
          if (!sessionId) {
            setDraftRuntimeOverride(next);
            return;
          }
          setPending(next);
        }}
      />
      {foreign && shownModel ? (
        <span className="composer__context-label" title={tr("composer.runtimeModel")}>
          {shownModel}
        </span>
      ) : null}
      <GlassModal
        open={pending !== null}
        onClose={() => setPending(null)}
        title={tr("composer.runtimeSwitchTitle")}
        size="sm"
        closeLabel={tr("common.close")}
        footer={
          <>
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => setPending(null)}
            >
              {tr("common.cancel")}
            </button>
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => {
                if (pending !== null) commit(pending);
                setPending(null);
              }}
            >
              {tr("common.confirm")}
            </button>
          </>
        }
      >
        <p>{tr("composer.runtimeSwitchBody")}</p>
      </GlassModal>
    </>
  );
}

export function sessionUsesForeignRuntime(
  file: AgentRuntimesFile | null,
  sessionId: string | null,
  draftPick?: string,
): boolean {
  if (!file) return false;
  const profileId = sessionId
    ? file.sessionBindings.find((row) => row.sessionId === sessionId)?.profileId ??
      ""
    : resolveDraftProfileId(draftPick, file.desktopDefaultProfileId);
  if (!profileId) return false;
  const profile = file.profiles.find((row) => row.id === profileId);
  return !!profile && profile.kind !== "atlas";
}
