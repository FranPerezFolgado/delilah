import type { BackgroundInfo, ConversationEntry } from "./api";

export type Outcome = "ok" | "failed" | "stopped" | "other";

/** How a finished background task ended, from its reported status and exit code. */
export function outcome(info: BackgroundInfo): Outcome {
  const s = info.status;
  if (s === "failed") return "failed";
  if (s === "stopped" || s === "killed") return "stopped";
  if (s === "completed") return info.exit_code != null && info.exit_code !== 0 ? "failed" : "ok";
  if (s == null && info.exit_code != null) return info.exit_code === 0 ? "ok" : "failed";
  return "other";
}

const bg = (e: ConversationEntry) => (e.kind === "marker" ? e.background : undefined);

/** Joins each notification whose report arrived as a hand-back message with that hand-back
 *  (same task id, nearest first, one to one). Pure: recomputed on every render. */
export function foldBackground(entries: ConversationEntry[]): ConversationEntry[] {
  const pairs: { n: number; h: number; d: number }[] = [];
  entries.forEach((n, i) => {
    const ni = bg(n);
    if (ni?.source !== "notification" || !ni.report_in_handback || !ni.task_id) return;
    entries.forEach((h, j) => {
      const hi = bg(h);
      if (hi?.source === "handback" && hi.task_id === ni.task_id) pairs.push({ n: i, h: j, d: Math.abs(i - j) });
    });
  });
  pairs.sort((a, b) => a.d - b.d || a.n - b.n || a.h - b.h);
  const taken = new Map<number, number>();
  const used = new Set<number>();
  for (const p of pairs) {
    if (taken.has(p.n) || used.has(p.h)) continue;
    taken.set(p.n, p.h);
    used.add(p.h);
  }
  const out: ConversationEntry[] = [];
  entries.forEach((e, i) => {
    if (used.has(i)) return;
    const h = taken.get(i);
    const ni = bg(e);
    const hi = h === undefined ? undefined : bg(entries[h]);
    if (e.kind === "marker" && ni && hi) {
      const { report, report_truncated } = hi;
      out.push({ ...e, background: { ...ni, report, report_truncated } });
    } else out.push(e);
  });
  return out;
}
