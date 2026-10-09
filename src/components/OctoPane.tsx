/**
 * Main-pane Octo: fixed embedded page. Desktop uses a child webview;
 * the browser preview falls back to the EmbeddedBrowser iframe.
 *
 * Once opened, the pane stays mounted and only hides. Unmounting would
 * close the child webview and reload the page on the next visit.
 */
import { useRef } from "react";
import { EmbeddedBrowser } from "@/components/EmbeddedBrowser";
import type { Locale } from "@/i18n";

export const OCTO_URL = "https://im.deepminer.com.cn/";

export function OctoPane({
  locale,
  title,
  active,
}: {
  locale: Locale;
  title: string;
  active: boolean;
}) {
  return (
    <div className="octo-pane" hidden={!active}>
      <EmbeddedBrowser
        url={OCTO_URL}
        title={title}
        locale={locale}
        instanceId="octo"
        active={active}
        showAddress={false}
      />
    </div>
  );
}

/** Mount Octo on first open and keep the webview alive across pane switches. */
export function OctoSlot({
  open,
  locale,
  title,
}: {
  open: boolean;
  locale: Locale;
  title: string;
}) {
  const keptRef = useRef(open);
  if (open) keptRef.current = true;
  if (!keptRef.current) return null;
  return <OctoPane locale={locale} title={title} active={open} />;
}
