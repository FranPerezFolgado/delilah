/** Seconds as "12 s" or "3 min 05 s". */
export function duration(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  if (s < 60) return `${s} s`;
  return `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")} s`;
}

const trim = (n: number) => n.toFixed(1).replace(/\.0$/, "");

export function formatTokens(n: number): string {
  if (n < 1000) return String(Math.round(n));
  if (n < 1_000_000) return `${trim(n / 1000)}K`;
  return `${trim(n / 1_000_000)}M`;
}
