// Every t("…") / tn(n, "…", "…") literal in the sources needs an English entry.
import { describe, expect, it } from "vitest";
import { EN } from "./en";

/** T097 turns this on: English keys that no source uses fail the test. */
const STRICT_ORPHANS = false;

const files = import.meta.glob<string>("../**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
});

const STR = String.raw`"((?:[^"\\\n]|\\.)*)"`;
const T_RE = new RegExp(String.raw`(?<![\w.$])t\(\s*${STR}`, "g");
const TN_RE = new RegExp(String.raw`(?<![\w.$])tn\(\s*[^,()]+,\s*${STR}\s*,\s*${STR}`, "g");

function used(): Map<string, string> {
  const found = new Map<string, string>();
  for (const [file, src] of Object.entries(files)) {
    if (/\.test\.tsx?$|\/i18n(\.ts$|\/en\/)/.test(file)) continue;
    const rel = file.replace(/^\.\.\//, "");
    const add = (raw: string) => found.set(JSON.parse(`"${raw}"`) as string, rel);
    for (const m of src.matchAll(T_RE)) add(m[1]);
    for (const m of src.matchAll(TN_RE)) {
      add(m[1]);
      add(m[2]);
    }
  }
  return found;
}

describe("i18n coverage", () => {
  it("every literal passed to t()/tn() has an English entry", () => {
    const missing = [...used()].filter(([es]) => EN[es] === undefined).map(([es, f]) => `${f}: ${es}`);
    expect(missing).toEqual([]);
  });

  it.runIf(STRICT_ORPHANS)("every English key is used by some source", () => {
    const u = used();
    expect(Object.keys(EN).filter((k) => !u.has(k))).toEqual([]);
  });
});
