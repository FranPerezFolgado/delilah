import { describe, expect, it } from "vitest";
import { reduce, type UpdateEvent, type UpdateMeta, type UpdateState } from "./updates";

const NOW = new Date("2026-10-24T10:00:00Z");
const meta = (over: Partial<UpdateMeta> = {}): UpdateMeta => ({
  installed: "0.4.0",
  lastCheck: null,
  lastResult: null,
  dismissed: null,
  ...over,
});
const avail = { kind: "available", version: "0.5.0", notes: "n" } as const;
const run = (s: UpdateState, e: UpdateEvent, m = meta()) => reduce(s, m, e, NOW);

describe("check", () => {
  it("starts checking from idle, keeping the manual flag", () => {
    expect(run({ kind: "idle" }, { type: "check", manual: true }).state).toEqual({ kind: "checking", manual: true });
    expect(run({ kind: "idle" }, { type: "check", manual: false }).state).toEqual({ kind: "checking", manual: false });
  });

  it("is ignored by an automatic check while downloading, waiting or installing", () => {
    const busy: UpdateState[] = [
      { kind: "downloading", version: "0.5.0", received: 0, total: null },
      { kind: "waiting", version: "0.5.0", notes: "n", running: 1 },
      { kind: "installing", version: "0.5.0" },
    ];
    for (const s of busy) {
      const r = run(s, { type: "check", manual: false });
      expect(r.state).toBe(s);
    }
  });

  it("is also ignored by a manual check while busy", () => {
    const s: UpdateState = { kind: "installing", version: "0.5.0" };
    expect(run(s, { type: "check", manual: true }).state).toBe(s);
  });
});

describe("check results", () => {
  const checking = (manual: boolean): UpdateState => ({ kind: "checking", manual });

  it("shows a found version and records the check", () => {
    const r = run(checking(false), { type: "found", version: "0.5.0", notes: "n" });
    expect(r.state).toEqual(avail);
    expect(r.meta.lastCheck).toBe(NOW);
    expect(r.meta.lastResult).toBe("available");
  });

  it("stays idle on an automatic re-check of the dismissed version", () => {
    const r = run(checking(false), { type: "found", version: "0.5.0", notes: "n" }, meta({ dismissed: "0.5.0" }));
    expect(r.state).toEqual({ kind: "idle" });
    expect(r.meta.lastResult).toBe("available");
  });

  it("shows the dismissed version on a manual check", () => {
    const r = run(checking(true), { type: "found", version: "0.5.0", notes: "n" }, meta({ dismissed: "0.5.0" }));
    expect(r.state).toEqual(avail);
  });

  it("shows a newer version than the dismissed one", () => {
    const r = run(checking(false), { type: "found", version: "0.6.0", notes: "n" }, meta({ dismissed: "0.5.0" }));
    expect(r.state.kind).toBe("available");
  });

  it("reports up to date only after a manual check", () => {
    const m = run(checking(true), { type: "none" });
    expect(m.state).toEqual({ kind: "upToDate", manual: true });
    expect(m.meta.lastResult).toBe("upToDate");
    expect(m.meta.lastCheck).toBe(NOW);
    expect(run(checking(false), { type: "none" }).state).toEqual({ kind: "idle" });
  });

  it("shows an error only after a manual check, but records it either way", () => {
    const m = run(checking(true), { type: "failure", reason: "offline" });
    expect(m.state).toEqual({ kind: "error", reason: "offline", manual: true });
    const a = run(checking(false), { type: "failure", reason: "offline" });
    expect(a.state).toEqual({ kind: "idle" });
    expect(a.meta.lastResult).toBe("error");
    expect(a.meta.lastCheck).toBe(NOW);
  });
});

describe("available", () => {
  it("'later' goes idle and remembers the version", () => {
    const r = run(avail, { type: "later" });
    expect(r.state).toEqual({ kind: "idle" });
    expect(r.meta.dismissed).toBe("0.5.0");
  });

  it("updates straight away with no running sessions", () => {
    expect(run(avail, { type: "update", running: 0 }).state).toEqual({
      kind: "downloading", version: "0.5.0", received: 0, total: null,
    });
  });

  it("asks for confirmation with running sessions", () => {
    expect(run(avail, { type: "update", running: 2 }).state).toEqual({
      kind: "confirmSessions", version: "0.5.0", notes: "n", running: 2,
    });
  });
});

describe("confirmSessions and waiting", () => {
  const confirm: UpdateState = { kind: "confirmSessions", version: "0.5.0", notes: "n", running: 2 };
  const waiting: UpdateState = { kind: "waiting", version: "0.5.0", notes: "n", running: 2 };

  it("'now' downloads, 'whenIdle' waits", () => {
    expect(run(confirm, { type: "now" }).state.kind).toBe("downloading");
    expect(run(confirm, { type: "whenIdle" }).state).toEqual(waiting);
  });

  it("cancel goes back to available from both", () => {
    expect(run(confirm, { type: "cancel" }).state).toEqual(avail);
    expect(run(waiting, { type: "cancel" }).state).toEqual(avail);
  });

  it("waiting follows the session count and downloads at zero", () => {
    expect(run(waiting, { type: "sessions", running: 1 }).state).toEqual({ ...waiting, running: 1 });
    expect(run(waiting, { type: "sessions", running: 0 }).state.kind).toBe("downloading");
  });

  it("waiting downloads on 'now'", () => {
    expect(run(waiting, { type: "now" }).state.kind).toBe("downloading");
  });
});

describe("downloading and installing", () => {
  const dl: UpdateState = { kind: "downloading", version: "0.5.0", received: 0, total: null };

  it("tracks progress", () => {
    expect(run(dl, { type: "progress", received: 5, total: 10 }).state).toEqual({ ...dl, received: 5, total: 10 });
  });

  it("moves to installing when done", () => {
    expect(run(dl, { type: "done" }).state).toEqual({ kind: "installing", version: "0.5.0" });
  });

  it("an error keeps the version and 'Reintentar' downloads again", () => {
    for (const s of [dl, { kind: "installing", version: "0.5.0" } as UpdateState]) {
      const e = run(s, { type: "failure", reason: "disk" }).state;
      expect(e).toEqual({ kind: "error", reason: "disk", manual: true, version: "0.5.0" });
      expect(run(e, { type: "retry" }).state).toEqual({ kind: "downloading", version: "0.5.0", received: 0, total: null });
    }
  });

  it("retry does nothing on an error without a version", () => {
    const e: UpdateState = { kind: "error", reason: "x", manual: true };
    expect(run(e, { type: "retry" }).state).toBe(e);
  });
});

describe("close", () => {
  it("returns upToDate and error to idle", () => {
    expect(run({ kind: "upToDate", manual: true }, { type: "close" }).state).toEqual({ kind: "idle" });
    expect(run({ kind: "error", reason: "x", manual: true }, { type: "close" }).state).toEqual({ kind: "idle" });
  });
});

describe("installed", () => {
  it("records the installed version without touching the state", () => {
    const r = run(avail, { type: "installed", version: "0.4.1" });
    expect(r.state).toBe(avail);
    expect(r.meta.installed).toBe("0.4.1");
  });
});
