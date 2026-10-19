// The desktop side of the inbox: the count on the Dock icon, a notification for each
// new item while you are elsewhere, and, when you come back to Delilah (e.g. from the
// Dock), the last item notified opens. Tauri's notifications report no clicks on
// desktop, so coming back is the signal.
import { useEffect, useRef } from "react";
import { BLOCKING, type InboxItem, type InboxKind, type Task } from "./api";
import { notify, setBadge } from "./platform";

const SAYS: Record<InboxKind, string> = {
  permission: "necesita un permiso",
  question: "te pregunta",
  error: "error en el turno",
  waiting: "te espera",
  review: "lista para revisar",
  paused: "pausada por límite",
  failed: "ha fallado",
};

/** Coming back later than this does not jump anywhere: it is no longer "the" news. */
const FRESH_MS = 10 * 60 * 1000;

const idOf = (i: InboxItem) => `${i.kind}:${i.task}:${i.session?.id ?? ""}:${i.since ?? ""}`;

/** First line of the agent's message, without Markdown marks, short enough for a banner. */
function gist(text: string | null, fallback: string): string {
  const line = (text ?? "").split("\n").find((l) => l.trim()) ?? "";
  const plain = line.replace(/[*`#>_]/g, "").trim();
  if (!plain) return fallback;
  return plain.length > 160 ? `${plain.slice(0, 157)}…` : plain;
}

export function useDesktopAlerts(inbox: InboxItem[], tasks: Task[], open: (item: InboxItem) => void) {
  const seen = useRef<Set<string> | null>(null);
  const statuses = useRef<Map<string, string> | null>(null);
  const pending = useRef<{ item: InboxItem; at: number } | null>(null);
  const openRef = useRef(open);
  openRef.current = open;

  useEffect(() => {
    setBadge(inbox.filter((i) => BLOCKING.includes(i.kind)).length).catch(() => {});
    const ids = new Set(inbox.map(idOf));
    // What was already there when the app opened is not news.
    if (seen.current) {
      for (const item of inbox) {
        if (seen.current.has(idOf(item)) || document.hasFocus()) continue;
        notify(`${item.task} ${SAYS[item.kind]}`, gist(item.text, item.title)).catch(() => {});
        pending.current = { item, at: Date.now() };
      }
    }
    seen.current = ids;
    // Dealt with elsewhere (e.g. answered from the terminal): nothing to jump to.
    if (pending.current && !ids.has(idOf(pending.current.item))) pending.current = null;
  }, [inbox]);

  // A task picking up again after the usage limit is good news too.
  useEffect(() => {
    const now = new Map(tasks.map((t) => [t.key, t.status.type]));
    if (statuses.current && !document.hasFocus()) {
      for (const t of tasks) {
        if (statuses.current.get(t.key) === "paused-by-limit" && t.status.type === "running") {
          notify(`${t.key} reanudada`, t.title).catch(() => {});
        }
      }
    }
    statuses.current = now;
  }, [tasks]);

  useEffect(() => {
    const onFocus = () => {
      const p = pending.current;
      pending.current = null;
      if (p && Date.now() - p.at < FRESH_MS) openRef.current(p.item);
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);
}
