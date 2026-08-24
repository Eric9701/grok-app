/**
 * Shared SSH watch state: enabled aliases, polled remote sessions, draft remote.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import * as api from "@/lib/api";

export type SshDraftRemote = {
  alias: string;
  path: string;
};

type Ctx = {
  watchAliases: string[];
  sessionsByAlias: Record<string, api.SshRemoteSession[]>;
  draftRemote: SshDraftRemote | null;
  setDraftRemote: (next: SshDraftRemote | null) => void;
  enableWatch: (alias: string) => Promise<api.SshWatchResult>;
  disableWatch: (alias: string) => Promise<api.SshWatchResult>;
  refreshSessions: (alias?: string) => Promise<void>;
};

const SshWatchContext = createContext<Ctx | null>(null);

const POLL_MS = 20_000;

export function SshWatchProvider({ children }: { children: ReactNode }) {
  const [watchAliases, setWatchAliases] = useState<string[]>([]);
  const [sessionsByAlias, setSessionsByAlias] = useState<
    Record<string, api.SshRemoteSession[]>
  >({});
  const [draftRemote, setDraftRemote] = useState<SshDraftRemote | null>(null);

  const hydrate = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      const s = await api.settingsGet();
      const aliases = (s.sshWatchAliases ?? []).filter(Boolean);
      setWatchAliases(aliases);
    } catch {
      /* soft-fail */
    }
  }, []);

  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  const refreshSessions = useCallback(async (alias?: string) => {
    if (!api.isTauri()) return;
    const targets = alias ? [alias] : watchAliases;
    if (targets.length === 0) return;
    const next: Record<string, api.SshRemoteSession[]> = {};
    await Promise.all(
      targets.map(async (a) => {
        try {
          const r = await api.sshListSessions(a);
          next[a] = r.ok ? r.sessions : [];
        } catch {
          next[a] = [];
        }
      }),
    );
    setSessionsByAlias((prev) => ({ ...prev, ...next }));
  }, [watchAliases]);

  useEffect(() => {
    if (watchAliases.length === 0) {
      setSessionsByAlias({});
      return;
    }
    void refreshSessions();
    const t = window.setInterval(() => {
      void refreshSessions();
    }, POLL_MS);
    return () => window.clearInterval(t);
  }, [watchAliases, refreshSessions]);

  const enableWatch = useCallback(async (alias: string) => {
    const r = await api.sshWatchStart(alias);
    if (r.ok) {
      setWatchAliases((prev) =>
        prev.includes(alias) ? prev : [...prev, alias],
      );
      void refreshSessions(alias);
    }
    return r;
  }, [refreshSessions]);

  const disableWatch = useCallback(async (alias: string) => {
    const r = await api.sshWatchStop(alias);
    if (r.ok) {
      setWatchAliases((prev) => prev.filter((a) => a !== alias));
      setSessionsByAlias((prev) => {
        const n = { ...prev };
        delete n[alias];
        return n;
      });
      setDraftRemote((cur) => (cur?.alias === alias ? null : cur));
    }
    return r;
  }, []);

  const value = useMemo<Ctx>(
    () => ({
      watchAliases,
      sessionsByAlias,
      draftRemote,
      setDraftRemote,
      enableWatch,
      disableWatch,
      refreshSessions,
    }),
    [
      watchAliases,
      sessionsByAlias,
      draftRemote,
      enableWatch,
      disableWatch,
      refreshSessions,
    ],
  );

  return (
    <SshWatchContext.Provider value={value}>{children}</SshWatchContext.Provider>
  );
}

export function useSshWatch(): Ctx {
  const ctx = useContext(SshWatchContext);
  if (!ctx) {
    return {
      watchAliases: [],
      sessionsByAlias: {},
      draftRemote: null,
      setDraftRemote: () => {},
      enableWatch: async (alias) => ({
        ok: false,
        alias,
        watching: false,
        error: "no provider",
      }),
      disableWatch: async (alias) => ({
        ok: true,
        alias,
        watching: false,
      }),
      refreshSessions: async () => {},
    };
  }
  return ctx;
}
