import { RAMPS } from "./pixtuoid";

// Port of pixtuoid's Rgb::ramp (OKLab lightness steps), see scripts/port-pixtuoid.py.
const LIGHTNESS_STEP = 0.1;
const HUE_PULL = 0.006;
const WARM_HUE_DEG = 80.0;
const COOL_HUE_DEG = 280.0;
const GAMUT_JND = 0.02;
const GAMUT_EPSILON = 0.0001;
const MAX_RAMP_LEVEL = 10;

type V3 = [number, number, number];

const toLinear = (c: number) => {
  c /= 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};
const fromLinear = (c: number) => {
  c = Math.min(1, Math.max(0, c));
  const v = c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055;
  return Math.floor(v * 255 + 0.5);
};
const cbrt = (x: number) => Math.sign(x) * Math.abs(x) ** (1 / 3);

function linearToOklab([r, g, b]: V3): V3 {
  const l = cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}
const rgbToOklab = (rgb: V3) => linearToOklab(rgb.map(toLinear) as V3);

function oklabToLinear([L, a, b]: V3): V3 {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}
const lchToLinear = (L: number, c: number, h: number) => oklabToLinear([L, c * Math.cos(h), c * Math.sin(h)]);
const within = (lin: V3) => lin.every((c) => c >= 0 && c <= 1);
const clamp01 = (lin: V3) => lin.map((c) => Math.min(1, Math.max(0, c))) as V3;
const dist = (p: V3, q: V3) => Math.sqrt(p.reduce((s, x, i) => s + (x - q[i]) ** 2, 0));

function fromOklabWithin([L, a, b]: V3, slack: number): V3 {
  if (L >= 1) return [255, 255, 255];
  if (L <= 0) return [0, 0, 0];
  const chroma = Math.hypot(a, b);
  const hue = Math.atan2(b, a);

  const clip = (ch: number) => {
    const clipped = clamp01(lchToLinear(L, ch, hue));
    const landed = linearToOklab(clipped);
    const miss = dist(landed, [L, ch * Math.cos(hue), ch * Math.sin(hue)]);
    return { clipped, miss, near: miss < GAMUT_JND && Math.abs(landed[0] - L) < slack };
  };

  let clipped = lchToLinear(L, chroma, hue);
  if (!within(clipped)) {
    let r = clip(chroma);
    clipped = r.clipped;
    if (!r.near) {
      let lo = 0;
      let hi = chroma;
      let minInGamut = true;
      while (hi - lo > GAMUT_EPSILON) {
        const cur = (lo + hi) / 2;
        if (minInGamut && within(lchToLinear(L, cur, hue))) {
          lo = cur;
          continue;
        }
        r = clip(cur);
        clipped = r.clipped;
        if (!r.near) hi = cur;
        else if (GAMUT_JND - r.miss < GAMUT_EPSILON) break;
        else {
          minInGamut = false;
          lo = cur;
        }
      }
    }
  }
  return clipped.map(fromLinear) as V3;
}

function rampStep(base: V3, steps: number, lit: boolean, prev: V3, prevAim: number): [V3, number] {
  const keep = (1 - LIGHTNESS_STEP) ** steps;
  let own: number;
  let hue: number;
  let toward: number;
  if (lit) [own, hue, toward] = [1 - (1 - base[0]) * keep, WARM_HUE_DEG, 1];
  else [own, hue, toward] = [base[0] * keep, COOL_HUE_DEG, -1];
  const rad = (hue * Math.PI) / 180;
  const pull = HUE_PULL * steps;
  const a = base[1] + pull * Math.cos(rad);
  const b = base[2] + pull * Math.sin(rad);
  const room = lit ? 1 - prevAim : prevAim;
  const shares = Math.max(MAX_RAMP_LEVEL - steps, 0) + 2;
  const gap = Math.min(GAMUT_JND, room / shares);
  const aim = lit ? Math.max(own, prevAim + gap) : Math.min(own, prevAim - gap);
  const below = rgbToOklab(prev)[0];
  let l = aim;
  let nudge = GAMUT_EPSILON;
  for (;;) {
    const c = fromOklabWithin([l, a, b], gap / 2);
    const lightness = rgbToOklab(c)[0];
    const past = lit ? lightness > below : lightness < below;
    if (past || (lit && l >= 1) || (!lit && l <= 0)) return [c, aim];
    l += toward * nudge;
    nudge *= 2;
  }
}

const parse = (h: string): V3 => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)) as V3;
const toHex = (rgb: V3) => "#" + rgb.map((c) => c.toString(16).padStart(2, "0")).join("");

/** `hex` moved `level` OKLab lightness steps (positive lighter and warmer, negative darker and cooler). */
export function ramp(hex: string, level: number): string {
  if (level === 0) return hex.toLowerCase();
  const rgb = parse(hex);
  const lit = level > 0;
  const base = rgbToOklab(rgb);
  let prev = rgb;
  let aim = base[0];
  for (let steps = 1; steps <= Math.abs(level); steps++) [prev, aim] = rampStep(base, steps, lit, prev, aim);
  return toHex(prev);
}

/** Palette override for a look: B/P/H/S plus every ramp key derived from them. */
export function recolour(base: { B: string; P: string; H: string; S: string }): Record<string, string> {
  const out: Record<string, string> = { ...base };
  for (const [key, def] of Object.entries(RAMPS)) {
    const b = (base as Record<string, string>)[def.of];
    if (b) out[key] = ramp(b, def.level);
  }
  return out;
}
