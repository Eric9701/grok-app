/**
 * Settings → Runtime → SSH: watch list, live search, available hosts.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
import type { Vars } from "@/i18n";
import { UiSwitch } from "./shared";
import { partitionSshHosts } from "@/lib/sshHostMatch";
import { useSshWatch } from "@/providers/SshWatchProvider";

type Props = {
  t: (k: string, vars?: Vars) => string;
};

export function SshHostsPanel({ t }: Props) {
  const watch = useSshWatch();
  const [list, setList] = useState<api.SshListResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [testing, setTesting] = useState<string | null>(null);
  const [toggling, setToggling] = useState<string | null>(null);
  const [probes, setProbes] = useState<Record<string, api.SshProbeResult>>({});
  const [copied, setCopied] = useState<string | null>(null);
  const [query, setQuery] = useState("");

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

  const runTest = async (alias: string): Promise<api.SshProbeResult | null> => {
    if (!api.isTauri()) return null;
    setTesting(alias);
    try {
      const r = await api.sshTestHost(alias);
      setProbes((prev) => ({ ...prev, [alias]: r }));
      return r;
    } catch (e) {
      const r: api.SshProbeResult = {
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
      };
      setProbes((prev) => ({ ...prev, [alias]: r }));
      return r;
    } finally {
      setTesting(null);
    }
  };

  const watchingSet = useMemo(
    () => new Set(watch.watchAliases),
    [watch.watchAliases],
  );
  const parts = useMemo(
    () => partitionSshHosts(list?.hosts ?? [], watchingSet, query),
    [list?.hosts, watchingSet, query],
  );

  const onWatch = async (alias: string, next: boolean) => {
    if (!api.isTauri() || toggling) return;
    setToggling(alias);
    try {
      if (next) {
        let probe = probes[alias];
        if (!probe?.sshOk) {
          probe = (await runTest(alias)) ?? probe;
        }
        if (!probe?.sshOk) return;
        if (probe.cli !== "ok") return;
        await watch.enableWatch(alias);
      } else {
        await watch.disableWatch(alias);
      }
    } finally {
      setToggling(null);
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
            disabled={!desktop}
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
        <>
          <HostSection
            title={t("settings.ssh.watchingTitle")}
            hint={t("settings.ssh.watchingHint")}
            empty={t("settings.ssh.watchingEmpty")}
            hosts={parts.watching}
            t={t}
            watching
            sshFound={!!list?.sshFound}
            testing={testing}
            toggling={toggling}
            probes={probes}
            copied={copied}
            sessionsByAlias={watch.sessionsByAlias}
            onTest={(a) => void runTest(a)}
            onWatch={onWatch}
            onCopy={copy}
          />

          <div className="settings-ssh-search">
            <input
              className="settings-input"
              type="search"
              value={query}
              placeholder={t("settings.ssh.searchPlaceholder")}
              aria-label={t("settings.ssh.searchPlaceholder")}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>

          {query.trim() &&
          parts.watching.length === 0 &&
          parts.available.length === 0 ? (
            <div className="settings-row__hint">{t("settings.ssh.searchEmpty")}</div>
          ) : (
            <HostSection
              title={t("settings.ssh.availableTitle")}
              hint={t("settings.ssh.availableHint")}
              empty={t("settings.ssh.availableEmpty")}
              hosts={parts.available}
              t={t}
              watching={false}
              sshFound={!!list?.sshFound}
              testing={testing}
              toggling={toggling}
              probes={probes}
              copied={copied}
              sessionsByAlias={watch.sessionsByAlias}
              onTest={(a) => void runTest(a)}
              onWatch={onWatch}
              onCopy={copy}
            />
          )}
        </>
      )}
    </div>
  );
}

function HostSection({
  title,
  hint,
  empty,
  hosts,
  t,
  watching,
  sshFound,
  testing,
  toggling,
  probes,
  copied,
  sessionsByAlias,
  onTest,
  onWatch,
  onCopy,
}: {
  title: string;
  hint: string;
  empty: string;
  hosts: api.SshHost[];
  t: (k: string, vars?: Vars) => string;
  watching: boolean;
  sshFound: boolean;
  testing: string | null;
  toggling: string | null;
  probes: Record<string, api.SshProbeResult>;
  copied: string | null;
  sessionsByAlias: Record<string, api.SshRemoteSession[]>;
  onTest: (alias: string) => void;
  onWatch: (alias: string, next: boolean) => void;
  onCopy: (key: string, text: string) => void;
}) {
  return (
    <div className="settings-ssh-section">
      <div className="settings-row__label">{title}</div>
      <div className="settings-row__hint">{hint}</div>
      {hosts.length === 0 ? (
        <div className="settings-row__hint">{empty}</div>
      ) : (
        <ul className="settings-ssh-list">
          {hosts.map((h) => {
            const probe = probes[h.alias];
            const busy = testing === h.alias || toggling === h.alias;
            const meta = [
              h.user ? t("settings.ssh.user", { user: h.user }) : null,
              h.hostname || null,
              h.port ? t("settings.ssh.port", { port: h.port }) : null,
            ]
              .filter(Boolean)
              .join(" · ");
            const remoteSessions = sessionsByAlias[h.alias] ?? [];
            return (
              <li key={h.alias} className="settings-ssh-host">
                <div className="settings-ssh-host__top">
                  <div className="settings-ssh-host__id">
                    <div className="settings-ssh-host__alias">{h.alias}</div>
                    {meta ? (
                      <div className="settings-ssh-host__meta">{meta}</div>
                    ) : null}
                  </div>
                  <div className="settings-ssh-host__actions">
                    <UiSwitch
                      checked={watching}
                      disabled={busy || !sshFound}
                      label={
                        watching
                          ? t("settings.ssh.watchOff")
                          : t("settings.ssh.watchOn")
                      }
                      onChange={(next) => onWatch(h.alias, next)}
                    />
                    <button
                      type="button"
                      className="btn btn--ghost btn--sm"
                      disabled={busy || !sshFound}
                      onClick={() => onTest(h.alias)}
                    >
                      {testing === h.alias
                        ? t("settings.ssh.testing")
                        : t("settings.ssh.test")}
                    </button>
                  </div>
                </div>
                {probe ? (
                  <HostProbe t={t} probe={probe} copied={copied} onCopy={onCopy} />
                ) : watching ? null : (
                  <div className="settings-row__hint">
                    {t("settings.ssh.notProbed")}
                  </div>
                )}
                {watching && remoteSessions.length > 0 ? (
                  <ul className="settings-ssh-sessions">
                    {remoteSessions.slice(0, 8).map((s) => (
                      <li key={s.id} className="settings-ssh-session">
                        <span className="settings-ssh-session__title">
                          {s.title}
                        </span>
                        {s.cwd ? (
                          <span className="settings-ssh-session__cwd">{s.cwd}</span>
                        ) : null}
                      </li>
                    ))}
                  </ul>
                ) : watching ? (
                  <div className="settings-row__hint">
                    {t("settings.ssh.remoteSessionsEmpty")}
                  </div>
                ) : null}
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
