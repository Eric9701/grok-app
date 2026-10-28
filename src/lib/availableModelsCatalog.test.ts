import { describe, expect, it } from "vitest";
import { catalogFromAvailableModels } from "./availableModelsCatalog";

describe("catalogFromAvailableModels", () => {
  it("maps host catalog rows and drops an empty payload", () => {
    expect(catalogFromAvailableModels({ models: [], defaultModelId: "" })).toEqual(
      [],
    );
    const catalog = catalogFromAvailableModels({
      defaultModelId: "glm-5",
      models: [
        {
          id: "glm-5",
          label: "glm-5",
          source: "config",
          isDefault: true,
          contextWindow: 128000,
          contextWindows: [128000, 0, -1],
        },
      ],
    });
    expect(catalog).toEqual([
      {
        id: "glm-5",
        label: "glm-5",
        source: "config",
        isDefault: true,
        reasoningEfforts: undefined,
        contextWindow: 128000,
        contextWindows: [128000],
      },
    ]);
  });
});
