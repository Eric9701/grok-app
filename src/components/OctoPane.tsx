/**
 * Main-pane Octo: fixed embedded page. Desktop uses a child webview;
 * the browser preview falls back to the EmbeddedBrowser iframe.
 */
import { EmbeddedBrowser } from "@/components/EmbeddedBrowser";
import type { Locale } from "@/i18n";

export const OCTO_URL = "https://im.deepminer.com.cn/";

export function OctoPane({
  locale,
  title,
}: {
  locale: Locale;
  title: string;
}) {
  return (
    <div className="octo-pane">
      <EmbeddedBrowser
        url={OCTO_URL}
        title={title}
        locale={locale}
        instanceId="octo"
        active
      />
    </div>
  );
}
