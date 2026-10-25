/** This process's launch chrome. Not stored in settings. */

import { useEffect, useState } from "react";
import { invoke, isDesktopHost } from "./host";

export interface LaunchChrome {
  /** False when the process was started with `--hide-remote`. */
  remote: boolean;
}

export async function launchChromeGet(): Promise<LaunchChrome> {
  if (!isDesktopHost()) return { remote: true };
  const chrome = await invoke<LaunchChrome>("launch_chrome_get");
  return { remote: chrome?.remote !== false };
}

/** Sidebar connect-device and Settings remote control share this switch. */
export function useRemoteChromeVisible(): boolean {
  const [visible, setVisible] = useState(true);
  useEffect(() => {
    if (!isDesktopHost()) return;
    let stop = false;
    void launchChromeGet()
      .then((chrome) => {
        if (!stop) setVisible(chrome.remote);
      })
      .catch(() => undefined);
    return () => {
      stop = true;
    };
  }, []);
  return visible;
}
