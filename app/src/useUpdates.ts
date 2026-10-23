// Owns the side effects of the update flow: the scheduled checks, the session count, the
// download and the relaunch. The decisions live in the pure reducer (updates.ts).
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { appVersion, canUpdate, checkForUpdate, relaunch, type FoundUpdate } from "./platform";
import { reduce, type UpdateEvent, type UpdateIntent, type UpdateMeta, type UpdateState } from "./updates";

const FIRST_CHECK_MS = 10_000;
const EVERY_MS = 6 * 60 * 60 * 1000;
const SESSIONS_POLL_MS = 10_000;

/** Sessions whose agent process is alive, working or waiting on the user. */
const countRunning = async () => (await api.sessions()).filter((s) => s.live && !s.closed).length;

export function useUpdates() {
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const [meta, setMeta] = useState<UpdateMeta>({ installed: "", lastCheck: null, lastResult: null, dismissed: null });
  // The reducer runs against a ref so that two events in a row see each other; the
  // React state mirrors it.
  const cur = useRef({ state, meta });
  const found = useRef<FoundUpdate | null>(null);
  const mounted = useRef(true);
  const installing = useRef(false);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const apply = useCallback((event: UpdateEvent): UpdateState => {
    const r = reduce(cur.current.state, cur.current.meta, event, new Date());
    cur.current = r;
    if (mounted.current) {
      setState(r.state);
      setMeta(r.meta);
    }
    return r.state;
  }, []);

  const install = useCallback(async () => {
    const update = found.current;
    if (!update) return;
    try {
      await update.install((received, total) => apply({ type: "progress", received, total }));
      apply({ type: "done" });
      await relaunch();
    } catch (e) {
      apply({ type: "failure", reason: (e as Error).message ?? String(e) });
    }
  }, [apply]);

  // The download starts once each time the flow enters `downloading`, whichever event got it there.
  useEffect(() => {
    if (state.kind !== "downloading") {
      installing.current = false;
    } else if (!installing.current) {
      installing.current = true;
      void install();
    }
  }, [state.kind, install]);

  // While waiting for sessions to finish, follow their count.
  useEffect(() => {
    if (state.kind !== "waiting") return;
    const id = setInterval(() => {
      countRunning()
        .then((running) => apply({ type: "sessions", running }))
        .catch(() => {}); // unknown count: keep waiting, never install
    }, SESSIONS_POLL_MS);
    return () => clearInterval(id);
  }, [state.kind, apply]);

  const dispatch = useCallback(
    (event: UpdateIntent) => {
      if (event.type !== "requestUpdate") {
        apply(event);
        return;
      }
      countRunning()
        .then((running) => apply({ type: "update", running }))
        .catch(() => {}); // without the count we can't tell whether sessions would be stopped
    },
    [apply],
  );

  const check = useCallback(
    async (manual: boolean) => {
      if (!canUpdate) return;
      if (apply({ type: "check", manual }).kind !== "checking") return;
      try {
        const update = await checkForUpdate();
        if (update) {
          found.current = update;
          apply({ type: "found", version: update.version, notes: update.notes });
        } else {
          apply({ type: "none" });
        }
      } catch (e) {
        apply({ type: "failure", reason: (e as Error).message ?? String(e) });
      }
    },
    [apply],
  );

  useEffect(() => {
    if (!canUpdate) return;
    let alive = true;
    appVersion()
      .then((v) => {
        if (!alive || !v) return;
        apply({ type: "installed", version: v });
      })
      .catch(() => {});
    const first = setTimeout(() => void check(false), FIRST_CHECK_MS);
    const every = setInterval(() => void check(false), EVERY_MS);
    return () => {
      alive = false;
      clearTimeout(first);
      clearInterval(every);
    };
  }, [check, apply]);

  return { state, meta, dispatch, check };
}
