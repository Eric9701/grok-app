/**
 * Settings → Runtime → SSH: list OpenSSH hosts, test, copy remote grok setup.
 */
import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import type { Vars } from "@/i18n";

type Props = {
  t: (k: string, vars?: Vars) => string;
};

export function SshHostsPanel({ t }: Props) {
  const [list, setList] = useState<api.SshListResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [testing, setTesting] = useState<string | null>(null);
  const [probes, setProbes] = useState<Record<string, api.SshProbeResult>>({});
  const [copied, setCopied] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setList(await api.sshListHosts());
    } catch (e) {
      setList({
        hosts: [],
        configPath: "",
        configExists: false,
        sshFound: false,
        error: String(e),
      });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const copy = async (key: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(key);
      window.setTimeout(() => setCopied(null), 1500);
    } catch {
      /* clipboard blocked */
    }
  };

  const runTest = async (alias: string) => {
    if (!api.isTauri()) return;
    setTesting(alias);
    try {
      const r = await api.sshTestHost(alias);
      setProbes((prev) => ({ ...prev, [alias]: r }));
    } catch (e) {
      setProbes((prev) => ({
        ...prev,
        [alias]: {
          alias,
          ok: false,
          sshOk: false,
          cli: "unknown",
          auth: "unknown",
          error: String(e),
          errorCode: "other",
          installCmd: `ssh ${alias} 'curl -fsSL https://x.ai/cli/install.sh | bash'`,
          loginCmd: `ssh -t ${alias} 'grok login --device-auth'`,
          installRemoteCmd: "curl -fsSL https://x.ai/cli/install.sh | bash",
          loginRemoteCmd: "grok login --device-auth",
        },
      }));
    } finally {
      setTesting(null);
    }
  };

  if (loading) {
    return (
      <div className="settings-card" id="settings-anchor-sshHosts">
        <div className="settings-row settings-row--stack">
          <div className="settings-row__hint">{t("settings.ssh.loading")}</div>
        </div>
      </div>
    );
  }

  const desktop = api.isTauri();
  const hosts = list?.hosts ?? [];

  return (
    <div className="settings-card" id="settings-anchor-sshHosts">
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("settings.ssh.title")}</div>
          <div className="settings-row__desc">{t("settings.ssh.desc")}</div>
        </div>
        <div className="settings-row__hint">{t("settings.ssh.honesty")}</div>
        <div className="settings-row__hint">{t("settings.ssh.batchMode")}</div>
        {list?.configPath ? (
          <div className="settings-row__hint">
            {t("settings.ssh.configPath", { path: list.configPath })}
          </div>
        ) : null}
        {!list?.sshFound ? (
          <div className="settings-row__hint is-danger" role="status">
            {t("settings.ssh.sshBinaryMissing")}
          </div>
        ) : null}
        {list?.error && list.error !== "desktop-only" ? (
          <div className="settings-row__hint is-danger" role="alert">
            {list.error}
          </div>
        ) : null}
        <div>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            disabled={!desktop || loading}
            onClick={() => void refresh()}
          >
            {t("settings.ssh.refresh")}
          </button>
        </div>
      </div>

      {!desktop ? (
        <div className="settings-row__hint">{t("settings.ssh.honesty")}</div>
      ) : !list?.configExists ? (
        <div className="settings-row__hint">
          {t("settings.ssh.configMissing", {
            path: list?.configPath || "~/.ssh/config",
          })}
        </div>
      ) : hosts.length === 0 ? (
        <div className="settings-row__hint">{t("settings.ssh.noHosts")}</div>
      ) : (
        <ul className="settings-ssh-list">
          {hosts.map((h) => {
            const probe = probes[h.alias];
            const busy = testing === h.alias;
            const meta = [
              h.user ? t("settings.ssh.user", { user: h.user }) : null,
              h.hostname || null,
              h.port ? t("settings.ssh.port", { port: h.port }) : null,
              h.identityFile
                ? t("settings.ssh.identity", { path: h.identityFile })
                : null,
            ]
              .filter(Boolean)
              .join(" · ");
            return (
              <li key={h.alias} className="settings-ssh-host">
                <div className="settings-ssh-host__top">
                  <div className="settings-ssh-host__id">
                    <div className="settings-ssh-host__alias">{h.alias}</div>
                    {meta ? (
                      <div className="settings-ssh-host__meta">{meta}</div>
                    ) : null}
                  </div>
                  <button
                    type="button"
                    className="btn btn--ghost"
                    disabled={busy || !list?.sshFound}
                    onClick={() => void runTest(h.alias)}
                  >
                    {busy ? t("settings.ssh.testing") : t("settings.ssh.test")}
                  </button>
                </div>
                {!probe ? (
                  <div className="settings-row__hint">{t("settings.ssh.notProbed")}</div>
                ) : (
                  <HostProbe t={t} probe={probe} copied={copied} onCopy={copy} />
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function HostProbe({
  t,
  probe,
  copied,
  onCopy,
}: {
  t: (k: string, vars?: Vars) => string;
  probe: api.SshProbeResult;
  copied: string | null;
  onCopy: (key: string, text: string) => void;
}) {
  const sshChip = probe.sshOk ? "is-ok" : "is-fail";
  const cliChip =
    probe.cli === "ok" ? "is-ok" : probe.cli === "missing" ? "is-fail" : "is-muted";
  const authChip =
    probe.auth === "ok" ? "is-ok" : probe.auth === "missing" ? "is-fail" : "is-muted";
  const showInstall = probe.cli !== "ok";
  const showLogin = probe.cli === "ok" && probe.auth !== "ok";

  return (
    <div className="settings-ssh-probe">
      <div className="settings-ssh-chips">
        <span className={"settings-acp-chip " + sshChip} role="status">
          <span className="settings-acp-chip__dot" aria-hidden />
          <span className="settings-acp-chip__label">
            {probe.sshOk
              ? t("settings.ssh.statusSshOk")
              : t("settings.ssh.statusSshFail", {
                  error: probe.error || t("settings.ssh.unknownHost"),
                })}
          </span>
          {probe.latencyMs != null ? (
            <span className="settings-acp-chip__meta">{probe.latencyMs} ms</span>
          ) : null}
        </span>
        {probe.sshOk ? (
          <>
            <span className={"settings-acp-chip " + cliChip} role="status">
              <span className="settings-acp-chip__dot" aria-hidden />
              <span className="settings-acp-chip__label">
                {probe.cli === "ok"
                  ? t("settings.ssh.statusCliOk", {
                      version: probe.cliVersion || "grok",
                    })
                  : t("settings.ssh.statusCliMissing")}
              </span>
            </span>
            <span className={"settings-acp-chip " + authChip} role="status">
              <span className="settings-acp-chip__dot" aria-hidden />
              <span className="settings-acp-chip__label">
                {probe.auth === "ok"
                  ? t("settings.ssh.statusAuthOk")
                  : t("settings.ssh.statusAuthMissing")}
              </span>
            </span>
          </>
        ) : null}
      </div>
      {probe.errorCode === "host_key" ? (
        <div className="settings-row__hint">
          {t("settings.ssh.hostKeyHint", { alias: probe.alias })}
        </div>
      ) : null}
      {showInstall ? (
        <CopyCmd
          title={t("settings.ssh.installTitle")}
          hint={t("settings.ssh.installHint", { alias: probe.alias })}
          cmd={probe.installCmd}
          copyKey={`${probe.alias}-install`}
          copied={copied}
          onCopy={onCopy}
          t={t}
        />
      ) : null}
      {showLogin ? (
        <CopyCmd
          title={t("settings.ssh.loginTitle")}
          hint={t("settings.ssh.loginHint", { alias: probe.alias })}
          cmd={probe.loginCmd}
          copyKey={`${probe.alias}-login`}
          copied={copied}
          onCopy={onCopy}
          t={t}
        />
      ) : null}
    </div>
  );
}

function CopyCmd({
  title,
  hint,
  cmd,
  copyKey,
  copied,
  onCopy,
  t,
}: {
  title: string;
  hint: string;
  cmd: string;
  copyKey: string;
  copied: string | null;
  onCopy: (key: string, text: string) => void;
  t: (k: string, vars?: Vars) => string;
}) {
  return (
    <div className="settings-ssh-cmd">
      <div className="settings-row__label">{title}</div>
      <div className="settings-row__hint">{hint}</div>
      <code className="settings-acp-cmd">{cmd}</code>
      <button
        type="button"
        className="btn btn--ghost btn--sm"
        onClick={() => onCopy(copyKey, cmd)}
      >
        {copied === copyKey ? t("message.copied") : t("message.copy")}
      </button>
    </div>
  );
}
