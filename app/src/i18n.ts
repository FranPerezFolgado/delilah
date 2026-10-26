// The app's language. Texts are written in Spanish in the components, wrapped in
// t("…"); the English ones live in i18n/en.ts, keyed by the Spanish text. A text with
// no translation shows in Spanish rather than breaking. The language is the
// preference when it is "es" or "en"; anything else follows the system, and English
// is the fallback for any system that is not Spanish.
import { EN } from "./i18n/en";

export type Lang = "es" | "en";
/** What the user picked in Ajustes: "auto" follows the Mac's language. */
export type LangSetting = "auto" | Lang;

/** The stored `language` preference as a setting: "es"/"en" as is, anything else "auto". */
export function parseSetting(v: unknown): LangSetting {
  return v === "es" || v === "en" ? v : "auto";
}

let current: Lang = "en";

/** The rule is in specs/008-public-repo-onboarding/contracts/language.md. */
export function detect(setting: unknown): Lang {
  if (setting === "es" || setting === "en") return setting;
  const system = typeof navigator !== "undefined" ? navigator.language : undefined;
  return typeof system === "string" && system.toLowerCase().startsWith("es") ? "es" : "en";
}

export function setLanguage(setting: unknown) {
  current = detect(setting);
  if (typeof document !== "undefined") document.documentElement.lang = current;
}

setLanguage("auto");

export const lang = () => current;

/** For dates and times: "es-ES" or "en-GB" (24-hour clock in both). */
export const locale = () => (current === "en" ? "en-GB" : "es-ES");

const missing = new Set<string>();

/** `es` in the current language; `{name}` holes are filled from `vars`. */
export function t(es: string, vars?: Record<string, string | number>): string {
  let text = es;
  if (current === "en") {
    const en = EN[es];
    if (en !== undefined) text = en;
    else if (import.meta.env.DEV && !missing.has(es)) {
      missing.add(es);
      console.warn(`[i18n] sin traducir: ${JSON.stringify(es)}`);
    }
  }
  if (vars) text = text.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  return text;
}

/** Singular or plural by `n`, e.g. tn(n, "{n} archivo", "{n} archivos"). */
export const tn = (n: number, one: string, many: string) => t(n === 1 ? one : many, { n });
