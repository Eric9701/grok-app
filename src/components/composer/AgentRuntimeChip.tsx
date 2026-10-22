/**
 * Composer chip: which CLI this desktop chat runs.
 * Hidden until at least one runtime profile exists.
 */
import { useEffect, useState } from "react";
import { GlassModal } from "@/components/GlassModal";
import { Select, type SelectOption } from "@/components/Select";
import * as api from "@/lib/api";
import {
  AGENT_RUNTIMES_EVENT,
  type AgentRuntimesFile,
} from "@/lib/api/agentRuntimes";
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

export function AgentRuntimeChip({
  sessionId,
  busy,
  tr,
}: {
  sessionId: string | null;
  busy: boolean;
  tr: TFn;
}) {
  const file = useAgentRuntimesFile();
  const [pending, setPending] = useState<string | null>(null);

  if (!file || file.profiles.length === 0 || !sessionId) return null;

  const current =
    file.sessionBindings.find((row) => row.sessionId === sessionId)?.profileId ??
    "";
  const currentProfile = file.profiles.find((profile) => profile.id === current);
  const foreign = !!currentProfile && currentProfile.kind !== "atlas";
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
    void api.agentRuntimesSetSessionBinding(sessionId, profileId);
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
          setPending(next);
        }}
      />
      {foreign && currentProfile?.model ? (
        <span className="composer__context-label" title={tr("composer.runtimeModel")}>
          {currentProfile.model}
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
): boolean {
  if (!file || !sessionId) return false;
  const binding = file.sessionBindings.find((row) => row.sessionId === sessionId);
  if (!binding) return false;
  const profile = file.profiles.find((row) => row.id === binding.profileId);
  return !!profile && profile.kind !== "atlas";
}
