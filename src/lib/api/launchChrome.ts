/** This process's launch chrome. Not stored in settings. */

import { useEffect, useState } from "react";
import { invoke, isDesktopHost } from "./host";

export interface LaunchChrome {
  /** True only when this process was started with `--show-remote`. */
  remote: boolean;
}

export async function launchChromeGet(): Promise<LaunchChrome> {
  if (!isDesktopHost()) return { remote: false };
  const chrome = await invoke<LaunchChrome>("launch_chrome_get");
  return { remote: chrome?.remote === true };
}

/** Sidebar connect-device and Settings remote control share this switch. */
export function useRemoteChromeVisible(): { visible: boolean; ready: boolean } {
  const desktop = isDesktopHost();
  const [state, setState] = useState({ visible: false, ready: !desktop });
  useEffect(() => {
    if (!desktop) return;
    let stop = false;
    void launchChromeGet()
      .then((chrome) => {
        if (!stop) setState({ visible: chrome.remote, ready: true });
      })
      .catch(() => {
        if (!stop) setState({ visible: false, ready: true });
      });
    return () => {
      stop = true;
    };
  }, [desktop]);
  return state;
}
