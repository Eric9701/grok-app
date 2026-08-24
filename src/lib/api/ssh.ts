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
