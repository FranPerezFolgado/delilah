// The daemon's change feed (`/api/v1/events`): it says what changed and each part of
// the UI fetches that again. The connection comes back by itself; when it does,
// everything is fetched again, as messages may have been missed meanwhile.
import { DAEMON_URL } from "./api";

export type Topic = "tasks" | "sessions" | "inbox" | "specs" | "usage" | "projects" | "prs" | "conversations";

const listeners = new Map<Topic, Set<() => void>>();

/** Calls `cb` whenever `topic` changes (and after a reconnection). Returns the unsubscribe. */
export function on(topic: Topic, cb: () => void): () => void {
  if (!listeners.has(topic)) listeners.set(topic, new Set());
  listeners.get(topic)!.add(cb);
  return () => listeners.get(topic)!.delete(cb);
}

const ALL: Topic[] = ["tasks", "sessions", "inbox", "specs", "usage", "projects", "prs", "conversations"];

// Bursts (a transition emits tasks and inbox; a hook may add sessions) are merged
// into one fetch per topic.
const pending = new Set<Topic>();
let flushTimer: ReturnType<typeof setTimeout> | null = null;
function notify(topics: Topic[]) {
  topics.forEach((t) => pending.add(t));
  if (flushTimer) return;
  flushTimer = setTimeout(() => {
    flushTimer = null;
    const due = [...pending];
    pending.clear();
    for (const t of due) listeners.get(t)?.forEach((cb) => cb());
  }, 60);
}

/** Opens the feed and keeps it open. `onStatus` tells whether it is connected. */
export function connect(onStatus: (connected: boolean) => void): () => void {
  let ws: WebSocket | null = null;
  let retry = 500;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const open = () => {
    ws = new WebSocket(`${DAEMON_URL.replace(/^http/, "ws")}/api/v1/events`);
    ws.onopen = () => {
      retry = 500;
      onStatus(true);
      notify(ALL);
    };
    ws.onmessage = (e) => {
      try {
        const { type } = JSON.parse(e.data as string) as { type: Topic | "resync" };
        notify(type === "resync" ? ALL : [type]);
      } catch {
        // Not a topic: ignore.
      }
    };
    ws.onclose = () => {
      onStatus(false);
      if (stopped) return;
      // Back off up to 5 s while the daemon is away (restarting, updating).
      timer = setTimeout(open, retry);
      retry = Math.min(retry * 2, 5000);
    };
  };
  open();
  return () => {
    stopped = true;
    if (timer) clearTimeout(timer);
    ws?.close();
  };
}
