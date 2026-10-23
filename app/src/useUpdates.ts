// Owns the side effects of the update flow: the scheduled checks, the download and the
// relaunch. The decisions live in the pure reducer (updates.ts).
import { useCallback, useEffect, useRef, useState } from "react";
import { appVersion, canUpdate, checkForUpdate, relaunch, type FoundUpdate } from "./platform";
import { reduce, type UpdateEvent, type UpdateMeta, type UpdateState } from "./updates";

const FIRST_CHECK_MS = 10_000;
const EVERY_MS = 6 * 60 * 60 * 1000;

export function useUpdates() {
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const [meta, setMeta] = useState<UpdateMeta>({ installed: "", lastCheck: null, lastResult: null, dismissed: null });
  // The reducer runs against refs so that two events in a row see each other.
  const cur = useRef({ state, meta });
  const found = useRef<FoundUpdate | null>(null);

  const apply = useCallback((event: UpdateEvent): UpdateState => {
    const r = reduce(cur.current.state, cur.current.meta, event, new Date());
    cur.current = r;
    setState(r.state);
    setMeta(r.meta);
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

  const dispatch = useCallback(
    (event: UpdateEvent) => {
      const before = cur.current.state.kind;
      const next = apply(event);
      if ((event.type === "update" || event.type === "retry" || event.type === "now" || event.type === "sessions") &&
        next.kind === "downloading" && before !== "downloading") {
        void install();
      }
    },
    [apply, install],
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
        cur.current = { ...cur.current, meta: { ...cur.current.meta, installed: v } };
        setMeta(cur.current.meta);
      })
      .catch(() => {});
    const first = setTimeout(() => void check(false), FIRST_CHECK_MS);
    const every = setInterval(() => void check(false), EVERY_MS);
    return () => {
      alive = false;
      clearTimeout(first);
      clearInterval(every);
    };
  }, [check]);

  return { state, meta, dispatch, check };
}
