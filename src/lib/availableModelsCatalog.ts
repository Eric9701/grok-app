import type { AvailableModelsResult } from "@/lib/api/settings";
import { positiveContextWindows } from "@/lib/contextWindowCommand";
import type { EffortOption, ModelOption } from "@/lib/grokCatalog";

/** Map a host catalog payload into the composer / settings model list. */
export function catalogFromAvailableModels(
  res: AvailableModelsResult | null | undefined,
): ModelOption[] {
  if (!res?.models?.length) return [];
  return res.models.map((m) => {
    const efforts: EffortOption[] | undefined = m.reasoningEfforts?.length
      ? m.reasoningEfforts.map((e) => ({
          id: e.id,
          value: e.value,
          label: e.label,
          description: e.description,
          isDefault: e.isDefault,
        }))
      : undefined;
    return {
      id: m.id,
      label: m.label || m.id,
      source: m.source,
      isDefault: m.isDefault,
      reasoningEfforts: efforts,
      contextWindow: m.contextWindow ?? null,
      contextWindows: positiveContextWindows(m.contextWindows),
    };
  });
}
