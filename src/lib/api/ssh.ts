/** API domain: SSH remote hosts (Settings → Runtime → SSH). */

import { invoke, isTauri } from "./host";

export type SshHost = {
  alias: string;
  hostname?: string | null;
  user?: string | null;
  port?: number | null;
  identityFile?: string | null;
};

export type SshListResult = {
  hosts: SshHost[];
  configPath: string;
  configExists: boolean;
  sshFound: boolean;
  error?: string | null;
};

export type SshProbeResult = {
  alias: string;
  ok: boolean;
  sshOk: boolean;
  cli: "ok" | "missing" | "unknown" | string;
  auth: "ok" | "missing" | "unknown" | string;
  cliPath?: string | null;
  cliVersion?: string | null;
  error?: string | null;
  errorCode?: string | null;
  latencyMs?: number | null;
  installCmd: string;
  loginCmd: string;
  installRemoteCmd: string;
  loginRemoteCmd: string;
};

export async function sshListHosts(): Promise<SshListResult> {
  if (!isTauri()) {
    return {
      hosts: [],
      configPath: "",
      configExists: false,
      sshFound: false,
      error: "desktop-only",
    };
  }
  return invoke<SshListResult>("ssh_list_hosts");
}

export async function sshTestHost(alias: string): Promise<SshProbeResult> {
  return invoke<SshProbeResult>("ssh_test_host", { alias });
}

export type SshWatchResult = {
  ok: boolean;
  alias: string;
  watching: boolean;
  error?: string | null;
  errorCode?: string | null;
};

export type SshDirEntry = {
  name: string;
  isDir: boolean;
};

export type SshListDirResult = {
  ok: boolean;
  alias: string;
  path: string;
  entries: SshDirEntry[];
  error?: string | null;
};

export type SshRemoteSession = {
  id: string;
  cwd: string;
  title: string;
  updatedAt?: string | null;
};

export type SshListSessionsResult = {
  ok: boolean;
  alias: string;
  sessions: SshRemoteSession[];
  error?: string | null;
};

export async function sshWatchStart(alias: string) {
  return invoke<SshWatchResult>("ssh_watch_start", { alias });
}

export async function sshWatchStop(alias: string) {
  return invoke<SshWatchResult>("ssh_watch_stop", { alias });
}

export async function sshListDir(alias: string, path?: string | null) {
  return invoke<SshListDirResult>("ssh_list_dir", {
    alias,
    path: path ?? null,
  });
}

export async function sshListSessions(alias: string) {
  return invoke<SshListSessionsResult>("ssh_list_sessions", { alias });
}
