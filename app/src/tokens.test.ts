/// <reference types="node" />
// The v2 token blocks of styles.css (dark `:root`, light in `prefers-color-scheme`):
// same names in both, and the WCAG contrast pairs of brief §1 «Contrast» (>= 4.5:1).
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

// vitest blanks `?raw` css imports, so read the file from disk.
const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

const src = css.replace(/\/\*[\s\S]*?\*\//g, "");
const LIGHT_RE = /@media\s*\(prefers-color-scheme:\s*light\)\s*\{\s*:root\s*\{([^}]*)\}\s*\}/;

function parse(block: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of block.matchAll(/(--[\w-]+)\s*:\s*([^;]+);?/g)) out[m[1]] = m[2].trim();
  return out;
}

const lightBlock = src.match(LIGHT_RE)?.[1] ?? "";
const darkBlock = [...src.replace(LIGHT_RE, "").matchAll(/:root\s*\{([^}]*)\}/g)]
  .map((m) => m[1])
  .find((b) => /--bg\s*:/.test(b)) ?? "";
const THEMES: Record<string, Record<string, string>> = {
  dark: parse(darkBlock),
  light: parse(lightBlock),
};

function lum(hex: string): number {
  const h = hex.replace("#", "");
  const [r, g, b] = [0, 2, 4].map((i) => {
    const c = parseInt(h.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function ratio(a: string, b: string): number {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const SURFACES = ["--bg", "--side", "--panel", "--card", "--dialog", "--sel"];
const PAIRS: [string, string][] = [
  ...["--tx", "--tx-strong", "--tx2"].flatMap((f) => SURFACES.map((s): [string, string] => [f, s])),
  // --tx3 only on bg, side and panel; on card/sel use --tx2 or stronger.
  ...["--bg", "--side", "--panel"].map((s): [string, string] => ["--tx3", s]),
  ...["wait", "run", "pause", "ok", "err", "neutral"].map((n): [string, string] => [`--${n}-tx`, `--${n}-bg`]),
  ["--primary-tx", "--primary"],
  ["--add-tx", "--add-bg"],
  ["--del-tx", "--del-bg"],
  ["--term-tx", "--term-bg"],
  // text of the hot counter (.cnt.hot) on amber
  ["--wait-on", "--wait"],
];

const NAMES = [
  ...SURFACES, "--hov", "--bd", "--bd2", "--tx", "--tx-strong", "--tx2", "--tx3",
  "--primary", "--primary-hov", "--primary-tx", "--focus",
  ...["wait", "run", "pause", "ok", "err", "neutral"].flatMap((n) => [`--${n}`, `--${n}-bg`, `--${n}-tx`]),
  "--wait-on", "--add-bg", "--add-tx", "--del-bg", "--del-tx", "--hunk-bg",
  "--term-bg", "--term-tx", "--shadow", "--scrim",
];

/**
 * PENDING owner decision: pairs of the design under 4.5:1. Reported, not changed here; the
 * entry must still be below 4.5 (so it is deleted when the design value is fixed).
 */
const KNOWN_LOW: Record<string, string> = {
  "light --tx3 on --side": "#6b6e75 on #efefec is 4.43:1 in the appendix values",
};

describe("v2 tokens", () => {
  for (const [theme, tokens] of Object.entries(THEMES)) {
    it(`${theme}: defines every token and its color-scheme`, () => {
      expect(NAMES.filter((n) => !(n in tokens))).toEqual([]);
    });

    it(`${theme}: colour tokens are plain hex`, () => {
      const bad = NAMES.filter((n) => !["--shadow", "--scrim"].includes(n) && !/^#[0-9a-f]{6}$/i.test(tokens[n] ?? ""));
      expect(bad).toEqual([]);
    });

    it(`${theme}: contrast >= 4.5:1 on every text pair`, () => {
      const low = PAIRS.flatMap(([fg, bg]) => {
        const r = ratio(tokens[fg], tokens[bg]);
        return r >= 4.5 || `${theme} ${fg} on ${bg}` in KNOWN_LOW ? [] : [`${fg} on ${bg}: ${r.toFixed(2)}`];
      });
      expect(low).toEqual([]);
    });
  }

  it("pending: known-low pairs are still below 4.5:1", () => {
    const stale = Object.keys(KNOWN_LOW).filter((k) => {
      const [theme, fg, , bg] = k.split(" ");
      return ratio(THEMES[theme][fg], THEMES[theme][bg]) >= 4.5;
    });
    expect(stale).toEqual([]);
  });

  it("declares color-scheme in each block", () => {
    expect(darkBlock).toMatch(/color-scheme:\s*dark/);
    expect(lightBlock).toMatch(/color-scheme:\s*light/);
  });
});
