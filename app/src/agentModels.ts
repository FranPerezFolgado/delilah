import { t } from "./i18n";
import type { AgentModel } from "./api";

/** The name of a model in the pickers. */
export function modelLabel(m: AgentModel): string {
  switch (m) {
    case "opus": return "Opus";
    case "sonnet": return "Sonnet";
    case "opusplan": return "Opus plan";
    case "haiku": return "Haiku";
    case "default": return t("Por defecto de Claude");
  }
}
