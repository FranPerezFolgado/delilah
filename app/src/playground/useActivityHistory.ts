import { useEffect, useRef, useState } from "react";
import { api, type HistoryResponse } from "../api";
import { connect, on } from "../live";

type Sessions = HistoryResponse["sessions"];

/** Folds an incremental response into what is known: new entries (seq above the session's
 *  last) are appended, `live`/`dropped` are updated, absent sessions are kept. */
function merge(prev: Sessions, next: Sessions): Sessions {
  const out = new Map(prev.map((s) => [s.session, s]));
  for (const s of next) {
    const old = out.get(s.session);
    const last = old?.entries.at(-1)?.seq ?? -1;
    const fresh = s.entries.filter((e) => e.seq > last);
    out.set(s.session, { ...s, entries: old ? [...old.entries, ...fresh] : fresh });
  }
  return [...out.values()];
}

const maxSeq = (sessions: Sessions) =>
  sessions.reduce((m, s) => Math.max(m, s.entries.at(-1)?.seq ?? 0), 0);

/** The activity history of `owner` (feeds the Flujo view): a full fetch on mount, then only
 *  what is after the last seq on each `activity` change; a full one again after a reconnect.
 *  Returns `startedAt` (the response's `started_at`), the per-session `sessions` and `stale`
 *  (the feed is down: the last picture is kept). `online` given is used as is; without it,
 *  this opens its own connection (connect() is not idempotent), as usePlayground does. */
export function useActivityHistory(
  owner: string,
  online?: boolean,
): { startedAt: string | null; sessions: Sessions; stale: boolean } {
  const [startedAt, setStartedAt] = useState<string | null>(null);
  const [sessions, setSessions] = useState<Sessions>([]);
  const [own, setOwn] = useState(true);
  const external = online !== undefined;
  const connected = online ?? own;

  useEffect(() => {
    if (external) return;
    return connect(setOwn);
  }, [external]);

  const known = useRef<Sessions>([]);
  const requests = useRef(0);
  const applied = useRef(0);
  const needFull = useRef(true);
  const fetchRef = useRef<() => void>(() => {});

  useEffect(() => {
    let cancelled = false;
    known.current = [];
    applied.current = requests.current;
    needFull.current = true;
    setStartedAt(null);
    setSessions([]);
    const load = () => {
      const full = needFull.current;
      const id = ++requests.current;
      api
        .activityHistory(owner, full ? undefined : maxSeq(known.current))
        .then((r) => {
          // A slower, older response must not overwrite a newer state.
          if (cancelled || id < applied.current) return;
          applied.current = id;
          known.current = full ? r.sessions : merge(known.current, r.sessions);
          if (full) needFull.current = false;
          setStartedAt(r.started_at);
          setSessions(known.current);
        })
        .catch(() => {});
    };
    fetchRef.current = load;
    load();
    const off = on("activity", load);
    return () => {
      cancelled = true;
      off();
    };
  }, [owner]);

  // Back after a drop: messages may have been missed, so fetch everything again.
  const was = useRef(connected);
  useEffect(() => {
    if (connected && !was.current) {
      needFull.current = true;
      fetchRef.current();
    }
    was.current = connected;
  }, [connected]);

  return { startedAt, sessions, stale: !connected };
}
