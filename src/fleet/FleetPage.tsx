import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createT, type Locale } from "@/i18n";
import * as api from "@/lib/api";
import type { FleetModel, FleetStatus, FleetTask, FleetTrace } from "@/lib/api/fleet";
import { FleetBoard } from "./FleetBoard";
import { preferredAgent } from "./pickAgent";

export function FleetPage({ locale }: { locale: Locale }) {
  const tr = useMemo(() => createT(locale), [locale]);
  const [status, setStatus] = useState<FleetStatus | null>(null);
  const [tasks, setTasks] = useState<FleetTask[]>([]);
  const [trace, setTrace] = useState<FleetTrace | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);
  const [agentId, setAgentId] = useState("local");
  const [agentTouched, setAgentTouched] = useState(false);
  const [cwd, setCwd] = useState("");
  const [prompt, setPrompt] = useState("");
  const [models, setModels] = useState<FleetModel[]>([]);
  const [modelId, setModelId] = useState("");
  const [modelsBusy, setModelsBusy] = useState(false);
  const [modelsError, setModelsError] = useState("");
  const modelTouched = useRef(false);
  const modelsGen = useRef(0);
  const alive = useRef(true);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState("");

  const refresh = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      const [nextStatus, nextTasks] = await Promise.all([
        api.fleetStatus(),
        api.fleetTasks(),
      ]);
      setStatus(nextStatus);
      setTasks(nextTasks);
    } catch {
      /* keep the last snapshot */
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const loadModels = useCallback(async () => {
    if (!api.isTauri()) return;
    const mine = ++modelsGen.current;
    setModelsBusy(true);
    setModelsError("");
    try {
      const next = await api.fleetModels();
      if (!alive.current || mine !== modelsGen.current) return;
      setModels(next.models);
      setModelId((prev) => {
        const known = prev !== "" && next.models.some((model) => model.id === prev);
        if (modelTouched.current && (prev === "" || known)) return prev;
        if (
          next.currentModelId &&
          next.models.some((model) => model.id === next.currentModelId)
        ) {
          return next.currentModelId;
        }
        return "";
      });
    } catch (err) {
      if (!alive.current || mine !== modelsGen.current) return;
      const code = invokeError(err);
      setModelsError(
        code.includes("runtime_missing_executable")
          ? tr("fleet.cliMissing")
          : tr("fleet.modelRefreshFailed"),
      );
    } finally {
      if (alive.current && mine === modelsGen.current) setModelsBusy(false);
    }
  }, [tr]);

  useEffect(() => {
    void loadModels();
  }, [loadModels]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, 2000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    const next = preferredAgent(status?.machines ?? [], agentId, agentTouched);
    if (next) setAgentId(next);
  }, [status, agentId, agentTouched]);

  useEffect(() => {
    if (!openId || !api.isTauri()) return;
    let stop = false;
    const load = async () => {
      try {
        const next = await api.fleetTaskTrace(openId);
        if (!stop) setTrace(next);
      } catch {
        if (!stop) setTrace(null);
      }
    };
    void load();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void load();
    }, 2000);
    return () => {
      stop = true;
      window.clearInterval(timer);
    };
  }, [openId]);

  const dispatch = async () => {
    setFormError("");
    if (!api.isTauri()) return;
    setBusy(true);
    try {
      await api.fleetDispatch(agentId.trim(), cwd.trim(), prompt, modelId);
      setPrompt("");
      await refresh();
    } catch (err) {
      const code = invokeError(err);
      setFormError(
        code.includes("cwd_not_absolute")
          ? tr("fleet.cwdRelative")
          : code.includes("invalid_model")
            ? tr("fleet.modelInvalid")
            : tr("fleet.dispatchFailed"),
      );
    } finally {
      setBusy(false);
    }
  };

  const cancel = async (id: string) => {
    if (!api.isTauri()) return;
    try {
      await api.fleetCancel(id);
      await refresh();
    } catch {
      setFormError(tr("fleet.dispatchFailed"));
    }
  };

  return (
    <FleetBoard
      tr={tr}
      status={status}
      tasks={tasks}
      trace={trace}
      agentId={agentId}
      cwd={cwd}
      prompt={prompt}
      models={models}
      modelId={modelId}
      modelsBusy={modelsBusy}
      modelsError={modelsError}
      busy={busy}
      formError={formError}
      onAgentId={(id) => {
        setAgentTouched(true);
        setAgentId(id);
      }}
      onCwd={setCwd}
      onPrompt={setPrompt}
      onModelId={(id) => {
        modelTouched.current = true;
        setModelId(id);
      }}
      onRefreshModels={() => void loadModels()}
      onDispatch={() => void dispatch()}
      onCancel={(id) => void cancel(id)}
      onOpen={setOpenId}
      onCloseTrace={() => {
        setOpenId(null);
        setTrace(null);
      }}
    />
  );
}

function invokeError(err: unknown) {
  return typeof err === "string" ? err : err instanceof Error ? err.message : "";
}
