// Every t("…") / tn(n, "…", "…") literal in the sources needs an English entry.
import { describe, expect, it } from "vitest";
import { EN } from "./en";

/** English keys that no source uses fail the test. */
const STRICT_ORPHANS = true;

const files = import.meta.glob<string>("../**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
});

/** Tests, the English dictionary itself (the files next to this one) and i18n.ts are not sources. */
const isSkipped = (file: string) => /\.test\.tsx?$|\/i18n\.ts$|^\.\/en(\/|\.ts$)/.test(file);

const STR = String.raw`"((?:[^"\\\n]|\\.)*)"`;
const T_RE = new RegExp(String.raw`(?<![\w.$])t\(\s*${STR}`, "g");
const TN_RE = new RegExp(String.raw`(?<![\w.$])tn\(\s*[^,()]+,\s*${STR}\s*,\s*${STR}`, "g");

function used(): Map<string, string> {
  const found = new Map<string, string>();
  for (const [file, src] of Object.entries(files)) {
    if (isSkipped(file)) continue;
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

/**
 * Every plain string literal of the sources: a label kept in a table and passed to t()
 * later (`label: "Cerrada"` … `t(label)`) counts as used, so only truly dead keys are orphans.
 */
function literals(): Set<string> {
  const out = new Set<string>();
  const re = /"((?:[^"\\\n]|\\.)*)"|`([^`$\\]*)`/g;
  for (const [file, src] of Object.entries(files)) {
    if (isSkipped(file)) continue;
    for (const m of src.matchAll(re)) {
      try {
        out.add(m[1] !== undefined ? (JSON.parse(`"${m[1]}"`) as string) : m[2]);
      } catch {
        // not a JSON string (an odd escape): it cannot be a key
      }
    }
  }
  return out;
}

describe("i18n coverage", () => {
  it("finds the literals (the scan itself works)", () => {
    expect(used().size).toBeGreaterThan(100);
  });

  it("every literal passed to t()/tn() has an English entry", () => {
    const missing = [...used()].filter(([es]) => EN[es] === undefined).map(([es, f]) => `${f}: ${es}`);
    expect(missing).toEqual([]);
  });

  it.runIf(STRICT_ORPHANS)("every English key is used by some source", () => {
    const u = literals();
    for (const k of used().keys()) u.add(k);
    expect(Object.keys(EN).filter((k) => !u.has(k))).toEqual([]);
  });
});
