import { useCallback, useEffect, useState } from "react";
import * as api from "@/lib/api";
import { useSettingsModel } from "@/providers/SettingsModelContext";

function encodeQuery(raw: string) {
  return encodeURIComponent(raw).replace(
    /[!'()*]/g,
    (ch) => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`,
  );
}

/** Workers dial loopback when the leader is bound to every interface. */
export function fleetDialTarget(bind: string) {
  const target = bind.trim();
  if (target.startsWith("0.0.0.0:")) return `127.0.0.1:${target.slice("0.0.0.0:".length)}`;
  if (target === "[::]" || target.startsWith("[::]:")) {
    return target.replace("[::]", "[::1]");
  }
  return target;
}

/** Same shape the leader returns after a successful apply. */
export function fleetWorkerCommand(bind: string, agentId: string, token: string) {
  const target = fleetDialTarget(bind);
  const ws = `ws://${target}/ws?agent_id=${encodeQuery(agentId)}&token=${encodeQuery(token)}`;
  return `atlas agent --always-approve headless --grok-ws-url '${ws}' --grok-ws-origin 'http://${target}'`;
}

export function FleetListenCard() {
  const { t, rowHighlight } = useSettingsModel();
  const [enabled, setEnabled] = useState(false);
  const [bind, setBind] = useState("127.0.0.1:24720");
  const [token, setToken] = useState("");
  const [sampleAgent, setSampleAgent] = useState("worker-1");
  const [listening, setListening] = useState(false);
  const [error, setError] = useState("");
  const [command, setCommand] = useState("");
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    if (!api.isTauri()) return;
    try {
      const settings = await api.settingsGet();
      setEnabled(!!settings.fleetListenEnabled);
      setBind(settings.fleetBind?.trim() || "127.0.0.1:24720");
      setToken(settings.fleetToken ?? "");
      const status = await api.fleetStatus();
      setListening(status.listening);
      setError(status.listenError ?? "");
    } catch {
      /* keep the draft */
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const apply = async () => {
    if (!api.isTauri()) return;
    setBusy(true);
    setCopied(false);
    try {
      const result = await api.fleetListenApply(
        enabled,
        bind.trim(),
        token.trim(),
        sampleAgent.trim() || "worker-1",
      );
      setListening(result.listening);
      setError(result.error ?? "");
      setBind(result.bind || bind);
      setCommand(result.connectionCli ?? "");
      await refresh();
      if (result.connectionCli) setCommand(result.connectionCli);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("fleet.bindError"));
      setCommand("");
    } finally {
      setBusy(false);
    }
  };

  const shownCommand =
    command ||
    (token.trim()
      ? fleetWorkerCommand(bind.trim() || "127.0.0.1:24720", sampleAgent.trim() || "worker-1", token.trim())
      : "");

  const copy = async () => {
    if (!shownCommand) return;
    try {
      await navigator.clipboard.writeText(shownCommand);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div
      className={"settings-card" + rowHighlight("settings-anchor-fleetListen")}
      id="settings-anchor-fleetListen"
    >
      <div className="settings-row settings-row--stack">
        <div className="settings-row__text">
          <div className="settings-row__label">{t("fleet.listen")}</div>
          <div className="settings-row__desc">{t("fleet.listenDesc")}</div>
        </div>
        <p className="settings-row__desc" role="status">
          {error ? t("fleet.bindError") : listening ? t("fleet.listening") : enabled ? t("fleet.savedOnly") : t("fleet.listen")}
        </p>
        <label className="settings-row__desc">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(event) => setEnabled(event.target.checked)}
          />{" "}
          {t("fleet.enable")}
        </label>
        <label className="settings-row__desc">
          {t("fleet.bind")}
          <input
            className="settings-input"
            value={bind}
            onChange={(event) => setBind(event.target.value)}
            spellCheck={false}
            autoComplete="off"
            aria-label={t("fleet.bind")}
          />
        </label>
        <label className="settings-row__desc">
          {t("fleet.token")}
          <input
            className="settings-input"
            type="password"
            value={token}
            onChange={(event) => setToken(event.target.value)}
            spellCheck={false}
            autoComplete="off"
            aria-label={t("fleet.token")}
          />
        </label>
        <p className="settings-row__desc">{t("fleet.tokenHint")}</p>
        <label className="settings-row__desc">
          {t("fleet.sampleAgent")}
          <input
            className="settings-input"
            value={sampleAgent}
            onChange={(event) => setSampleAgent(event.target.value)}
            spellCheck={false}
            autoComplete="off"
            aria-label={t("fleet.sampleAgent")}
          />
        </label>
        {error ? (
          <p className="settings-row__desc" role="alert">
            {t("fleet.bindError")}
          </p>
        ) : null}
        <button type="button" className="btn btn--primary" disabled={busy} onClick={() => void apply()}>
          {busy ? t("fleet.applying") : t("fleet.apply")}
        </button>
        {shownCommand ? (
          <>
            <pre className="fleet-page__command">{shownCommand}</pre>
            <button type="button" className="btn" onClick={() => void copy()}>
              {copied ? t("fleet.copied") : t("fleet.copyCommand")}
            </button>
          </>
        ) : null}
      </div>
    </div>
  );
}
