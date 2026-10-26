/// <reference types="node" />
// Guards for styles.css and every co-located stylesheet (SC-001, SC-004, FR-001, FR-002):
// no raw colour outside the two token blocks, none of the old token names, and the
// type / radius scale of contracts/ui.md.
import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// vitest blanks `?raw` css imports, so the stylesheets are read from disk.
function cssFiles(dir: URL, prefix = "."): Record<string, string> {
  const out: Record<string, string> = {};
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.isDirectory()) Object.assign(out, cssFiles(new URL(`${e.name}/`, dir), `${prefix}/${e.name}`));
    else if (e.name.endsWith(".css")) out[`${prefix}/${e.name}`] = readFileSync(new URL(e.name, dir), "utf8");
  }
  return out;
}
const sheets = cssFiles(new URL("./", import.meta.url));
const sources = import.meta.glob<string>("./**/*.{ts,tsx}", { query: "?raw", import: "default", eager: true });

const strip = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, "");
const LIGHT_RE = /@media\s*\(prefers-color-scheme:\s*light\)\s*\{\s*:root\s*\{[^}]*\}\s*\}/;
const rel = (f: string) => f.replace(/^\.\//, "");

/** A stylesheet without comments and, for styles.css, without the two token blocks. */
function body(file: string, raw: string): string {
  if (rel(file) !== "styles.css") return strip(raw);
  let s = strip(raw).replace(LIGHT_RE, "");
  s = s.replace(/:root\s*\{[^}]*--bg\s*:[^}]*\}/, "");
  return s;
}

it("reads the stylesheets", () => {
  expect(Object.keys(sheets)).toContain("./styles.css");
  expect(sheets["./styles.css"].length).toBeGreaterThan(1000);
});

describe("no raw colours", () => {
  it("only the two token blocks of styles.css hold colour values", () => {
    const bad: string[] = [];
    for (const [file, raw] of Object.entries(sheets)) {
      const m = body(file, raw).match(/#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\(/g);
      if (m) bad.push(`${rel(file)}: ${m.join(" ")}`);
    }
    expect(bad).toEqual([]);
  });
});

const OLD_TOKENS = /--(?:acc|accbg|acctx|elev|done|pausebg|addbg|delbg|hunkbg)\b|--term(?![-\w])/;

/**
 * PENDING: files still using an old token because a later package owns them. An entry
 * must still violate (so the list cannot go stale) and must be deleted when its owner lands.
 */
const PENDING_OLD_TOKENS: Record<string, string> = {
};

describe("no old token names", () => {
  const all = { ...sheets, ...sources };
  const files = Object.entries(all).filter(([f]) => !/\.test\.tsx?$/.test(f));

  it("none left in app/src ts, tsx and css, outside the pending list", () => {
    const bad = files.filter(([f, s]) => !(rel(f) in PENDING_OLD_TOKENS) && OLD_TOKENS.test(s)).map(([f]) => rel(f));
    expect(bad).toEqual([]);
  });

  it("pending entries still violate (delete the entry when its package lands)", () => {
    const stale = Object.keys(PENDING_OLD_TOKENS).filter((p) => {
      const hit = files.find(([f]) => rel(f) === p);
      return !hit || !OLD_TOKENS.test(hit[1]);
    });
    expect(stale).toEqual([]);
  });
});

const FONT_SIZES = new Set([19, 15, 13.5, 12]);
const RADII = new Set([7, 10, 14, 11]);

/** Sub-element exceptions of contracts/ui.md: selector -> property -> allowed px values. */
const ALLOW: Record<string, { "font-size"?: number[]; "border-radius"?: number[] }> = {
  ".kbd": { "font-size": [11], "border-radius": [4] }, // keyboard hint chip
  ".cnt": { "font-size": [11], "border-radius": [9] }, // count bubble
  ".bars i": { "border-radius": [2] }, // phase bar segment
  ".dot": { "border-radius": [4] }, // status dot
  ".toggle": { "border-radius": [8] }, // toggle (font-size is 13.5 = base)
  ".toggle .sw": { "border-radius": [7] }, // toggle switch track
  ".toggle .sw::after": { "border-radius": [5] }, // toggle knob
  ".seg": { "border-radius": [8] }, // segmented control
  ".seg button": { "border-radius": [6] }, // segmented option
  ".menu button": { "border-radius": [6] }, // menu item
  ".menu a": { "border-radius": [6] }, // menu item (link)
  ".opt": { "border-radius": [9] }, // option card
  "::-webkit-scrollbar-thumb": { "border-radius": [5] }, // themed scrollbar thumb
  ".sk": { "border-radius": [6] }, // skeleton line
};

function violations(css: string): string[] {
  const out: string[] = [];
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const sels = m[1].split(",").map((s) => s.trim().replace(/\s+/g, " "));
    for (const d of m[2].split(";")) {
      const mm = d.match(/^\s*(font-size|border-radius)\s*:\s*(.+?)\s*$/);
      if (!mm) continue;
      const prop = mm[1] as "font-size" | "border-radius";
      const vals = mm[2].split(/\s+/);
      for (const v of vals) {
        const n = parseFloat(v);
        if (/^(0|inherit|initial|unset)$/.test(v)) continue;
        if (prop === "border-radius" && v === "50%") continue; // round dots and marks
        const ok = (prop === "font-size" ? FONT_SIZES : RADII).has(n) && v.endsWith("px");
        if (ok) continue;
        if (sels.every((s) => ALLOW[s]?.[prop]?.includes(n) && v.endsWith("px"))) continue;
        out.push(`${m[1].trim()} { ${prop}: ${v} }`);
      }
    }
  }
  return out;
}

describe("type and radius scale", () => {
  it("font sizes 19/15/13.5/12 and radii 7/10/14/11, plus the documented allow-list", () => {
    const bad = Object.entries(sheets).flatMap(([f, raw]) => violations(body(f, raw)).map((v) => `${rel(f)}: ${v}`));
    expect(bad).toEqual([]);
  });

  it("the pre-v2 LEGACY section of styles.css is gone", () => {
    expect(sheets["./styles.css"]).not.toContain("LEGACY");
  });

  it("round dots and marks use border-radius: 50%", () => {
    const round = ".sb-dot, .sb-hot, .tv-dot, .ttab-dot, .ttab-dirty, .preview-dot, .fr-mark".split(", ");
    const all = Object.values(sheets).map(strip).join("\n");
    for (const c of round) {
      const rules = [...all.matchAll(new RegExp(`${c.replace(".", "\\.")}\\b[^{}]*\\{([^{}]*)\\}`, "g"))].map((m) => m[1]);
      expect(rules.some((r) => /border-radius:\s*50%/.test(r)), c).toBe(true);
    }
  });
});
