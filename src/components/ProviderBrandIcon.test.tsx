import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ProviderBrandIcon } from "./ProviderBrandIcon";

describe("ProviderBrandIcon", () => {
  it("renders OpenRouter with currentColor so CSS can switch light/dark", () => {
    const html = renderToStaticMarkup(
      <ProviderBrandIcon brand="openrouter" title="OpenRouter" />,
    );
    expect(html).toContain("provider-brand-icon--openrouter");
    expect(html).toContain('fill="currentColor"');
    expect(html).toContain("<title>OpenRouter</title>");
    expect(html).not.toContain("#7624F4");
    expect(html).not.toContain("#7624f4");
  });
});
