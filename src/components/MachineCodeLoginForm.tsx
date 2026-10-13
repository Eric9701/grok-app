/**
 * Private atlas-server machine-code approval for an in-flight CLI device login.
 * The CLI still writes auth.json after the server accepts the code.
 */

import { useEffect, useState } from "react";
import type { MessageKey } from "@/i18n";
import * as api from "@/lib/api";

type Tr = (
  key: MessageKey,
  vars?: Record<string, string | number>,
) => string;

export function MachineCodeLoginForm({
  t,
  active,
}: {
  t: Tr;
  active: boolean;
}) {
  const [deviceUserCode, setDeviceUserCode] = useState<string | null>(null);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [machineCode, setMachineCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!active) {
      setDeviceUserCode(null);
      setBusy(false);
      setError(null);
    }
  }, [active]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void api
      .listen<{ userCode?: string }>("account://device-code", (payload) => {
        const code = payload?.userCode?.trim();
        if (code) {
          setDeviceUserCode(code);
          setError(null);
        }
      })
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (!active) return null;

  const ready =
    !!deviceUserCode &&
    !!email.trim() &&
    !!password &&
    !!machineCode.trim() &&
    !busy;

  return (
    <div className="account-machine">
      {!deviceUserCode ? <p>{t("account.machineWaiting")}</p> : null}
      {deviceUserCode ? (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (!ready || !deviceUserCode) return;
            void (async () => {
              setBusy(true);
              setError(null);
              try {
                await api.accountLoginApproveMachine({
                  email: email.trim(),
                  password,
                  machineCode: machineCode.trim(),
                });
              } catch (err) {
                setError(`${t("account.machineFailed")}: ${String(err)}`);
              } finally {
                setBusy(false);
              }
            })();
          }}
        >
          <p className="account-machine__code">
            {t("account.machineUserCode", { code: deviceUserCode })}
          </p>
          <p>{t("account.machineHint")}</p>
          <label className="account-machine__field">
            <span>{t("account.machineEmail")}</span>
            <input
              type="email"
              className="input"
              value={email}
              autoComplete="username"
              spellCheck={false}
              disabled={busy}
              onChange={(e) => setEmail(e.target.value)}
            />
          </label>
          <label className="account-machine__field">
            <span>{t("account.machinePassword")}</span>
            <input
              type="password"
              className="input"
              value={password}
              autoComplete="current-password"
              disabled={busy}
              onChange={(e) => setPassword(e.target.value)}
            />
          </label>
          <label className="account-machine__field">
            <span>{t("account.machineCode")}</span>
            <input
              type="text"
              className="input account-machine__code"
              value={machineCode}
              autoComplete="off"
              spellCheck={false}
              disabled={busy}
              onChange={(e) => setMachineCode(e.target.value)}
            />
          </label>
          {error ? <p className="account-login-help__err">{error}</p> : null}
          <button type="submit" className="btn btn--solid btn--sm" disabled={!ready}>
            {t("account.machineSubmit")}
          </button>
        </form>
      ) : null}
    </div>
  );
}
