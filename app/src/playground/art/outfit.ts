export interface Look {
  B: string;
  P: string;
  H: string;
  S: string;
}

const MASK = 0xffffffffffffffffn;

/** pixtuoid's splitmix64 finaliser, wrapping u64. */
export function splitmix64(input: bigint): bigint {
  let z = input & MASK;
  z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & MASK;
  z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & MASK;
  return z ^ (z >> 31n);
}

/** Seed of a string: splitmix64 of the UTF-8 bytes folded with h*131+b. */
export function stringSeed(s: string): bigint {
  let h = 0n;
  for (const b of new TextEncoder().encode(s)) h = (h * 131n + BigInt(b)) & MASK;
  return splitmix64(h);
}

const o = (B: string, P: string) => ({ B, P });

export const OUTFITS: { B: string; P: string }[] = [
  o("#eee1c6", "#4a2b3d"),
  o("#c97b5e", "#6b573d"),
  o("#c9a24b", "#4a5234"),
  o("#8a2c36", "#5a4e42"),
  o("#d77a61", "#27334a"),
  o("#b89968", "#3d2a1f"),
  o("#a54f2c", "#cdc0a3"),
  o("#e0907c", "#3a322e"),
  o("#a4b595", "#33363d"),
  o("#9bb5c8", "#3c4452"),
  o("#a290b0", "#3c2a1e"),
  o("#3f614c", "#7a6748"),
  o("#3e7a85", "#c7b696"),
  o("#3f4a75", "#8a847a"),
  o("#6b84a0", "#2a334a"),
  o("#47695a", "#b8ae95"),
];

export const HAIR_PRESETS = ["#140a06", "#2a1a0e", "#523210", "#8a5a36", "#c7a34a", "#d86832", "#7a3210", "#a8a8b0"];
export const SKIN_PRESETS = ["#f4c79a", "#e0a870", "#b88050", "#8a5a36", "#c89a64"];

/** Outfit from the project, hair and skin from the session. */
export function lookFor(project: string, sessionId: string): Look {
  const outfit = OUTFITS[Number(stringSeed(project) % 16n)];
  const s = stringSeed(sessionId);
  return {
    B: outfit.B,
    P: outfit.P,
    H: HAIR_PRESETS[Number((s / 7n) % 8n)],
    S: SKIN_PRESETS[Number((s / 13n) % 5n)],
  };
}

const ROLE_LOOKS: Record<string, Look> = {
  investigar: { ...OUTFITS[13], H: HAIR_PRESETS[1], S: SKIN_PRESETS[0] },
  implementar: { ...OUTFITS[5], H: HAIR_PRESETS[2], S: SKIN_PRESETS[1] },
  revisar: { ...OUTFITS[11], H: HAIR_PRESETS[7], S: SKIN_PRESETS[4] },
};
const GENERIC_LOOK: Look = { ...OUTFITS[8], H: HAIR_PRESETS[0], S: SKIN_PRESETS[2] };

export function roleLook(role: string): Look {
  return ROLE_LOOKS[role] ?? GENERIC_LOOK;
}
