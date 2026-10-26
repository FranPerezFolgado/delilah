#!/usr/bin/env python3
"""Port a subset of pixtuoid's 1x art to TypeScript.

Usage: python3 -I scripts/port-pixtuoid.py <pixtuoid checkout> <out dir>

Reads crates/pixtuoid-scene/sprites/default/pack.toml and the .sprite files of
the subset below, and writes <out dir>/pixtuoid.ts and <out dir>/ramp.fixtures.ts.
Python 3.11+, stdlib only. Fails (exit 1) on ragged rows, unknown keys, bad syntax.

pixtuoid is MIT, Copyright (c) 2026 Ivan Wang: https://github.com/ivanwang123/pixtuoid
(see app/src/playground/CREDITS.md).
"""
import json
import math
import sys
import tomllib
from pathlib import Path

SOURCE_URL = "https://github.com/ivanwang123/pixtuoid"
COMMIT = "90320b132ef0f5e844685e711f0820652c354dca"
PACK_DIR = "crates/pixtuoid-scene/sprites/default"

SUBSET = (
    ["seated", "typing_0", "typing_1", "standing", "seated_sleeping", "holding_coffee"]
    + [f"walking_{i}" for i in range(8)]
    + [f"walking_back_{i}" for i in range(8)]
    + ["desk", "desk_chair", "desk_cup", "token_tower"]
    + ["meeting_sofa", "side_table", "pantry_small", "water_cooler", "vending_machine"]
    + ["plant", "plant_tall", "whiteboard", "bookshelf", "wall_clock"]
    + ["icon_waiting", "icon_idle"]
)
# Keys the playground recolours per agent (shirt, pants, hair, skin).
RECOLOUR_BASES = ["B", "P", "H", "S"]


def die(msg):
    print(f"port-pixtuoid: error: {msg}", file=sys.stderr)
    sys.exit(1)


# ---- Rgb::ramp, ported from pixtuoid-core/src/sprite/mod.rs (OKLab) ----

RAMP_LIGHTNESS_STEP = 0.1
RAMP_HUE_PULL = 0.006
RAMP_WARM_HUE_DEG = 80.0
RAMP_COOL_HUE_DEG = 280.0
GAMUT_JND = 0.02
GAMUT_EPSILON = 0.0001
MAX_RAMP_LEVEL = 10


def to_linear(c):
    c /= 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def from_linear(c):
    c = min(1.0, max(0.0, c))
    v = 12.92 * c if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055
    return int(math.floor(v * 255.0 + 0.5))


def cbrt(x):
    return math.copysign(abs(x) ** (1 / 3), x)


def rgb_to_oklab(rgb):
    r, g, b = (to_linear(c) for c in rgb)
    l = cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
    m = cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
    s = cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
    return (
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    )


def oklab_to_linear(lab):
    L, a, b = lab
    l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3
    m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3
    s = (L - 0.0894841775 * a - 1.2914855480 * b) ** 3
    return (
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    )


def lch_to_linear(L, chroma, hue):
    return oklab_to_linear((L, chroma * math.cos(hue), chroma * math.sin(hue)))


def within_bounds(lin):
    return all(0.0 <= c <= 1.0 for c in lin)


def clamp01(lin):
    return tuple(min(1.0, max(0.0, c)) for c in lin)


def dist(p, q):
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(p, q)))


def linear_to_oklab(lin):
    r, g, b = lin
    l = cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
    m = cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
    s = cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
    return (
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    )


def from_oklab_within(lab, slack):
    L, a, b = lab
    if L >= 1.0:
        return (255, 255, 255)
    if L <= 0.0:
        return (0, 0, 0)
    chroma = math.hypot(a, b)
    hue = math.atan2(b, a)

    def clip(ch):
        clipped = clamp01(lch_to_linear(L, ch, hue))
        landed = linear_to_oklab(clipped)
        miss = dist(landed, (L, ch * math.cos(hue), ch * math.sin(hue)))
        near = miss < GAMUT_JND and abs(landed[0] - L) < slack
        return clipped, miss, near

    lin = lch_to_linear(L, chroma, hue)
    clipped = lin
    if not within_bounds(clipped):
        clipped, _, near = clip(chroma)
        if not near:
            lo, hi = 0.0, chroma
            min_in_gamut = True
            while hi - lo > GAMUT_EPSILON:
                cur = (lo + hi) / 2.0
                if min_in_gamut and within_bounds(lch_to_linear(L, cur, hue)):
                    lo = cur
                    continue
                clipped, miss, near = clip(cur)
                if not near:
                    hi = cur
                elif GAMUT_JND - miss < GAMUT_EPSILON:
                    break
                else:
                    min_in_gamut = False
                    lo = cur
    return tuple(from_linear(c) for c in clipped)


def ramp_step(base, steps, lit, prev, prev_aim):
    keep = (1.0 - RAMP_LIGHTNESS_STEP) ** steps
    if lit:
        own, hue, toward = 1.0 - (1.0 - base[0]) * keep, RAMP_WARM_HUE_DEG, 1.0
    else:
        own, hue, toward = base[0] * keep, RAMP_COOL_HUE_DEG, -1.0
    sin, cos = math.sin(math.radians(hue)), math.cos(math.radians(hue))
    pull = RAMP_HUE_PULL * steps
    a, b = base[1] + pull * cos, base[2] + pull * sin
    room = 1.0 - prev_aim if lit else prev_aim
    shares = max(MAX_RAMP_LEVEL - steps, 0) + 2
    gap = min(GAMUT_JND, room / shares)
    aim = max(own, prev_aim + gap) if lit else min(own, prev_aim - gap)
    below = rgb_to_oklab(prev)[0]
    l, nudge = aim, GAMUT_EPSILON
    while True:
        c = from_oklab_within((l, a, b), gap / 2.0)
        lightness = rgb_to_oklab(c)[0]
        past = lightness > below if lit else lightness < below
        if past or (lit and l >= 1.0) or (not lit and l <= 0.0):
            return c, aim
        l += toward * nudge
        nudge *= 2.0


def ramp(rgb, level):
    if level == 0:
        return rgb
    lit = level > 0
    base = rgb_to_oklab(rgb)
    prev, aim = rgb, base[0]
    for steps in range(1, abs(level) + 1):
        prev, aim = ramp_step(base, steps, lit, prev, aim)
    return prev


# ---- parsing ----


def hex_to_rgb(h, key):
    if not (len(h) == 7 and h[0] == "#"):
        die(f"palette key {key!r}: bad colour {h!r}")
    try:
        return tuple(int(h[i : i + 2], 16) for i in (1, 3, 5))
    except ValueError:
        die(f"palette key {key!r}: bad colour {h!r}")


def rgb_hex(rgb):
    return "#%02x%02x%02x" % rgb


def parse_sprite(path, name):
    """Return (frames: list[list[list[str]]], marks) of one .sprite file."""
    frames, marks = [], {}
    for n, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        where = f"{name}.sprite:{n}"
        if line.startswith("@frame"):
            frames.append([])
        elif line.startswith("@mark"):
            parts = line.split()
            if len(parts) != 4:
                die(f"{where}: bad @mark: {raw!r}")
            try:
                pos = (int(parts[2]), int(parts[3]))
            except ValueError:
                die(f"{where}: bad @mark coordinates: {raw!r}")
            if parts[1] in marks and marks[parts[1]] != pos:
                die(f"{where}: mark {parts[1]!r} differs between frames")
            marks[parts[1]] = pos
        elif line.startswith("@"):
            die(f"{where}: unknown directive: {raw!r}")
        else:
            toks = line.split()
            if any(len(t) != 1 for t in toks):
                die(f"{where}: row tokens must be one character: {raw!r}")
            if not frames:
                frames.append([])
            frames[-1].append(toks)
    if not frames or not any(frames):
        die(f"{name}.sprite: no pixels")
    for i, rows in enumerate(frames):
        if not rows:
            die(f"{name}.sprite: frame {i} is empty")
        if len({len(r) for r in rows}) != 1:
            die(f"{name}.sprite: frame {i} has ragged rows "
                f"(widths {sorted({len(r) for r in rows})})")
    if len({len(f[0]) for f in frames}) != 1:
        die(f"{name}.sprite: frames differ in width")
    return frames, marks


def safe_key(k):
    o = ord(k)
    return (
        o < 0xFFFF
        and not (0xD800 <= o <= 0xDFFF)
        and k.isprintable()
        and not k.isspace()
        and k not in '"\\\'`'
    )


def main():
    if len(sys.argv) != 3:
        die("usage: port-pixtuoid.py <pixtuoid checkout> <out dir>")
    root, out = Path(sys.argv[1]), Path(sys.argv[2])
    pack_dir = root / PACK_DIR
    if not (pack_dir / "pack.toml").is_file():
        die(f"{pack_dir / 'pack.toml'} not found")
    pack = tomllib.loads((pack_dir / "pack.toml").read_text(encoding="utf-8"))
    palette, ramps, anims = pack["palette"], pack["ramps"], pack["animations"]
    for k, v in palette.items():
        if len(k) != 1:
            die(f"palette key {k!r} is not one character")
        if k != ".":
            hex_to_rgb(v, k)
    if palette.get(".") != "transparent":
        die("palette '.' must be transparent")

    sprites = {}
    for name in SUBSET:
        f = pack_dir / f"{name}.sprite"
        if not f.is_file():
            die(f"{f} not found")
        sprites[name] = parse_sprite(f, name)

    # Keys used by the subset must be known.
    used = set()
    for name, (frames, _) in sprites.items():
        for rows in frames:
            for r in rows:
                for k in r:
                    if k != "." and k not in palette and k not in ramps:
                        die(f"{name}.sprite: unknown key {k!r}")
                    used.add(k)
    used.discard(".")

    # Keys to export: used ones, the recolour bases, and what the ramps need.
    keep_ramps = {k for k in ramps if k in used or ramps[k]["of"] in RECOLOUR_BASES}
    for k in keep_ramps:
        if ramps[k]["of"] not in palette:
            die(f"ramp {k!r} is of unknown key {ramps[k]['of']!r}")
    keep_palette = {k for k in used if k in palette} | set(RECOLOUR_BASES)
    keep_palette |= {ramps[k]["of"] for k in keep_ramps}
    all_keys = sorted(keep_palette | keep_ramps | {"."})

    # Remap keys that are not safe single BMP chars in a TS string.
    taken = set(all_keys)
    free = [chr(c) for c in range(0x21, 0x7F) if safe_key(chr(c)) and chr(c) not in taken]
    remap = {}
    for k in all_keys:
        if not safe_key(k) and k != ".":
            if not free:
                die("no free ASCII keys left to remap to")
            remap[k] = free.pop(0)
    rk = lambda k: remap.get(k, k)

    # Animations: non-density variants whose frames are all in the subset.
    sprite_names = set(sprites)
    animations = {}
    for name, a in anims.items():
        if "@" in name:
            continue
        files = [f[: -len(".sprite")] for f in a["frames"]]
        if all(f in sprite_names for f in files):
            animations[name] = (files, a["frame_ms"])

    # Fixtures: shade of each ramp key on the default palette.
    fixtures = {}
    for k in sorted(keep_ramps):
        base = hex_to_rgb(palette[ramps[k]["of"]], ramps[k]["of"])
        fixtures[k] = (ramps[k]["of"], ramps[k]["level"], rgb_hex(base),
                       rgb_hex(ramp(base, ramps[k]["level"])))

    # ---- write pixtuoid.ts ----
    j = lambda s: json.dumps(s, ensure_ascii=False)
    header = (
        f"// Pixel art ported from pixtuoid ({SOURCE_URL}), MIT, Copyright (c) 2026 Ivan Wang.\n"
        f"// Source commit {COMMIT}.\n"
        "// Generated by scripts/port-pixtuoid.py -- do not edit. See CREDITS.md.\n"
    )
    L = [header]
    L.append("export interface Sprite {")
    L.append("  w: number;")
    L.append("  h: number; // tallest frame")
    L.append("  /** One entry per `@frame`; each is an array of equal-width row strings (`.` is transparent). */")
    L.append("  frames: string[][];")
    L.append("  marks: Record<string, [number, number]>;")
    L.append("}")
    L.append("")
    L.append("export interface RampDef {")
    L.append("  of: string;")
    L.append("  level: number;")
    L.append("}")
    L.append("")
    L.append("export interface Animation {")
    L.append("  frames: string[]; // sprite names, in order")
    L.append("  frameMs: number;")
    L.append("}")
    L.append("")
    L.append("/** Palette key to hex colour (`.` is transparent and not listed). */")
    L.append("export const PALETTE: Record<string, string> = {")
    for k in sorted(keep_palette, key=rk):
        L.append(f"  {j(rk(k))}: {j(palette[k])},")
    L.append("};")
    L.append("")
    L.append("/** Derived shades: `level` OKLab steps along the ramp of palette key `of`. */")
    L.append("export const RAMPS: Record<string, RampDef> = {")
    for k in sorted(keep_ramps, key=rk):
        L.append(f"  {j(rk(k))}: {{ of: {j(rk(ramps[k]['of']))}, level: {ramps[k]['level']} }},")
    L.append("};")
    L.append("")
    L.append("export const SPRITES: Record<string, Sprite> = {")
    for name in SUBSET:
        frames, marks = sprites[name]
        w = len(frames[0][0])
        h = max(len(f) for f in frames)
        L.append(f"  {j(name)}: {{")
        L.append(f"    w: {w},")
        L.append(f"    h: {h},")
        L.append("    frames: [")
        for rows in frames:
            L.append("      [")
            for r in rows:
                L.append(f"        {j(''.join(rk(t) for t in r))},")
            L.append("      ],")
        L.append("    ],")
        mk = ", ".join(f"{j(m)}: [{x}, {y}]" for m, (x, y) in marks.items())
        L.append(f"    marks: {{ {mk} }}," if mk else "    marks: {},")
        L.append("  },")
    L.append("};")
    L.append("")
    L.append("export const ANIMATIONS: Record<string, Animation> = {")
    for name, (files, ms) in animations.items():
        fl = ", ".join(j(f) for f in files)
        L.append(f"  {j(name)}: {{ frames: [{fl}], frameMs: {ms} }},")
    L.append("};")
    L.append("")
    ts = "\n".join(L)

    # ---- write ramp.fixtures.ts ----
    F = [header]
    F.append("/** Shade of each ramp key on pixtuoid's default palette, from `Rgb::ramp`. */")
    F.append("export const RAMP_FIXTURES: Record<string, { of: string; level: number; base: string; shade: string }> = {")
    for k, (of, level, base, shade) in sorted(fixtures.items(), key=lambda kv: rk(kv[0])):
        F.append(f"  {j(rk(k))}: {{ of: {j(rk(of))}, level: {level}, base: {j(base)}, shade: {j(shade)} }},")
    F.append("};")
    F.append("")
    fx = "\n".join(F)

    out.mkdir(parents=True, exist_ok=True)
    (out / "pixtuoid.ts").write_text(ts, encoding="utf-8")
    (out / "ramp.fixtures.ts").write_text(fx, encoding="utf-8")
    print(f"sprites: {len(SUBSET)}, palette keys: {len(keep_palette)}, ramps: {len(keep_ramps)}, "
          f"animations: {len(animations)}, remapped: {remap or 'none'}, "
          f"pixtuoid.ts: {len(ts.encode())} bytes")


if __name__ == "__main__":
    main()
