// Owns the side effects of the update flow: the scheduled checks, the session count, the
// download and the relaunch. The decisions live in the pure reducer (updates.ts).
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { t } from "./i18n";
import { appVersion, canUpdate, checkForUpdate, onCheckUpdatesMenu, relaunch, type FoundUpdate } from "./platform";
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

  const check = useCallback(
    async (manual: boolean) => {
      if (!canUpdate) {
        // Dev builds and browsers answer a manual check instead of ignoring it.
        if (manual) apply({ type: "unavailable", reason: t("Las actualizaciones solo están en la app instalada") });
        return;
      }
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

  const requestUpdate = useCallback(() => {
    countRunning()
      .then((running) => apply({ type: "update", running }))
      // Without the count we can't tell whether sessions would be stopped: say so, never install.
      .catch((e) => apply({ type: "sessionsFailed", reason: (e as Error).message ?? String(e) }));
  }, [apply]);

  const dispatch = useCallback(
    (event: UpdateIntent) => {
      if (event.type === "requestUpdate") return requestUpdate();
      const { state: now } = cur.current;
      if (event.type === "retry" && now.kind === "error") {
        if (!now.version) return void check(true); // a failed manual check
        if (now.stage === "sessions") {
          apply(event);
          return requestUpdate();
        }
      }
      apply(event);
    },
    [apply, check, requestUpdate],
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
    // Scheduled checks honour the «auto-update» preference (read each time, so a change
    // applies at once); unreadable means on. A manual check never looks at it.
    const scheduled = async () => {
      const on = await api.preference<boolean>("auto-update").catch(() => true);
      if (alive && on !== false) void check(false);
    };
    const first = setTimeout(() => void scheduled(), FIRST_CHECK_MS);
    const every = setInterval(() => void scheduled(), EVERY_MS);
    return () => {
      alive = false;
      clearTimeout(first);
      clearInterval(every);
    };
  }, [check, apply]);

  // The app menu's "Buscar actualizaciones…".
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    onCheckUpdatesMenu(() => void check(true))
      .then((u) => (alive ? (unlisten = u) : u()))
      .catch(() => {});
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [check]);

  return { state, meta, dispatch, check };
}
