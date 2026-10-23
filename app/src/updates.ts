// The update flow as a pure state machine: the UI feeds it events (user choices, the
// result of a check, download progress) and renders the state. No Tauri here.

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking"; manual: boolean }
  | { kind: "upToDate"; manual: true }
  | { kind: "available"; version: string; notes: string }
  | { kind: "confirmSessions"; version: string; notes: string; running: number }
  | { kind: "waiting"; version: string; notes: string; running: number }
  | { kind: "downloading"; version: string; received: number; total: number | null }
  | { kind: "installing"; version: string }
  | { kind: "error"; reason: string; manual: boolean; version?: string };

export interface UpdateMeta {
  installed: string;
  lastCheck: Date | null;
  lastResult: "upToDate" | "available" | "error" | null;
  /** Version hidden with "Más tarde" until the next launch or manual check. */
  dismissed: string | null;
}

export type UpdateEvent =
  | { type: "check"; manual: boolean }
  | { type: "found"; version: string; notes: string }
  | { type: "none" }
  | { type: "failure"; reason: string }
  | { type: "later" }
  | { type: "update"; running: number }
  | { type: "now" }
  | { type: "whenIdle" }
  | { type: "cancel" }
  | { type: "sessions"; running: number }
  | { type: "progress"; received: number; total: number | null }
  | { type: "done" }
  | { type: "retry" }
  | { type: "close" }
  | { type: "installed"; version: string };

/** What the UI can ask for: the hook turns `requestUpdate` into `update` with the live session count. */
export type UpdateIntent = UpdateEvent | { type: "requestUpdate" };

export interface Reduced {
  state: UpdateState;
  meta: UpdateMeta;
}

const downloading = (version: string): UpdateState => ({ kind: "downloading", version, received: 0, total: null });

export function reduce(state: UpdateState, meta: UpdateMeta, event: UpdateEvent, now: Date): Reduced {
  const to = (next: UpdateState, m: UpdateMeta = meta): Reduced => ({ state: next, meta: m });
  const same = to(state);

  switch (event.type) {
    case "check":
      // Never while a download/install is in progress or another check is running.
      if (["checking", "downloading", "waiting", "installing", "confirmSessions"].includes(state.kind)) return same;
      // An automatic check does not disturb a visible offer.
      if (!event.manual && state.kind === "available") return same;
      return to({ kind: "checking", manual: event.manual });

    case "found":
      if (state.kind !== "checking") return same;
      {
        const m = { ...meta, lastCheck: now, lastResult: "available" as const };
        if (!state.manual && event.version === meta.dismissed) return to({ kind: "idle" }, m);
        return to(
          { kind: "available", version: event.version, notes: event.notes },
          state.manual ? { ...m, dismissed: null } : m,
        );
      }

    case "none":
      if (state.kind !== "checking") return same;
      return to(state.manual ? { kind: "upToDate", manual: true } : { kind: "idle" }, {
        ...meta, lastCheck: now, lastResult: "upToDate",
      });

    case "failure":
      if (state.kind === "checking") {
        const m = { ...meta, lastCheck: now, lastResult: "error" as const };
        return to(state.manual ? { kind: "error", reason: event.reason, manual: true } : { kind: "idle" }, m);
      }
      if (state.kind === "downloading" || state.kind === "installing") {
        return to({ kind: "error", reason: event.reason, manual: true, version: state.version });
      }
      return same;

    case "later":
      if (state.kind !== "available") return same;
      return to({ kind: "idle" }, { ...meta, dismissed: state.version });

    case "update":
      if (state.kind !== "available") return same;
      return to(
        event.running > 0
          ? { kind: "confirmSessions", version: state.version, notes: state.notes, running: event.running }
          : downloading(state.version),
      );

    case "now":
      if (state.kind === "confirmSessions" || state.kind === "waiting") return to(downloading(state.version));
      return same;

    case "whenIdle":
      if (state.kind !== "confirmSessions") return same;
      return to({ ...state, kind: "waiting" });

    case "cancel":
      if (state.kind !== "confirmSessions" && state.kind !== "waiting") return same;
      return to({ kind: "available", version: state.version, notes: state.notes });

    case "sessions":
      if (state.kind !== "waiting") return same;
      return to(event.running > 0 ? { ...state, running: event.running } : downloading(state.version));

    case "progress":
      if (state.kind !== "downloading") return same;
      return to({ ...state, received: event.received, total: event.total });

    case "done":
      if (state.kind !== "downloading") return same;
      return to({ kind: "installing", version: state.version });

    case "retry":
      if (state.kind !== "error" || !state.version) return same;
      return to(downloading(state.version));

    case "installed":
      return to(state, { ...meta, installed: event.version });

    case "close":
      if (state.kind !== "upToDate" && state.kind !== "error") return same;
      return to({ kind: "idle" });
  }
}
