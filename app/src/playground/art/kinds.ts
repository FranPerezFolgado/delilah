import type { ActionKind } from "../../api";

export const KIND_COLOURS: Record<ActionKind, string> = {
  edit: "#64a0ff",
  read: "#50dcf0",
  run: "#f0aa50",
  delegate: "#c88cff",
  other: "#8cf0aa",
};

/** Token-volume tier 0..3 used to scale the visuals. */
export function tokenTier(total: number): 0 | 1 | 2 | 3 {
  if (total >= 16_000_000) return 3;
  if (total >= 2_000_000) return 2;
  if (total >= 250_000) return 1;
  return 0;
}

const trim = (n: number) => n.toFixed(1).replace(/\.0$/, "");

export function formatTokens(n: number): string {
  if (n < 1000) return String(Math.round(n));
  if (n < 1_000_000) return `${trim(n / 1000)}K`;
  return `${trim(n / 1_000_000)}M`;
}
