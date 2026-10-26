import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { BLOCKING, type InboxKind, type PullRequest, type Session } from "./api";
import { setLanguage } from "./i18n";
import { kindOf, prStateOf, sessionDot, statusOf } from "./vocab";

setLanguage("es");

const ALL_KINDS: InboxKind[] = [
  "permission", "question", "error", "waiting", "review", "handoff", "suggest-review", "paused", "failed",
  "pr-failed", "pr-review", "pr-blocked", "branch", "autopilot", "base-stale", "turn-done", "pr-opened",
];

describe("kindOf", () => {
  it("BLOCKING equals the daemon fixture", () => {
    const fixture = JSON.parse(readFileSync("../crates/delilah-daemon/tests/fixtures/blocking_kinds.json", "utf8"));
    expect([...BLOCKING].sort()).toEqual([...fixture].sort());
  });
  it("maps every InboxKind; blocking matches BLOCKING; waiting kinds are not clearable", () => {
    for (const k of ALL_KINDS) {
      const v = kindOf(k);
      expect(["Pregunta", "Permiso", "Aviso"]).toContain(v.kind);
      expect(v.blocking).toBe(BLOCKING.includes(k));
      if (v.blocking) expect(v.clearable).toBe(false);
    }
    expect(kindOf("permission")).toEqual({ kind: "Permiso", blocking: true, clearable: false });
    expect(kindOf("waiting").kind).toBe("Pregunta");
    expect(kindOf("question").kind).toBe("Pregunta");
    expect(kindOf("pr-failed").kind).toBe("Aviso");
    expect(kindOf("turn-done")).toEqual({ kind: "Aviso", blocking: false, clearable: true });
    expect(kindOf("pr-opened").clearable).toBe(true);
  });
});

describe("statusOf", () => {
  it("maps statuses", () => {
    expect(statusOf("backlog")).toMatchObject({ tone: "neutral", label: "Backlog" });
    expect(statusOf({ type: "queued" })).toMatchObject({ tone: "neutral", label: "Backlog" });
    expect(statusOf({ type: "running" })).toMatchObject({ tone: "run", label: "Trabajando" });
    expect(statusOf({ type: "preparing-workspace" })).toMatchObject({ tone: "run", detail: "Preparando el workspace" });
    expect(statusOf({ type: "waiting-for-user" })).toMatchObject({ tone: "wait", label: "Te espera" });
    expect(statusOf({ type: "done" })).toMatchObject({ tone: "ok", label: "Hecha" });
    expect(statusOf({ type: "failed", reason: "boom" })).toMatchObject({ tone: "err", label: "Error", detail: "boom" });
    expect(statusOf({ type: "setup-failed", reason: "npm" })).toMatchObject({ tone: "err", label: "Error", detail: "npm" });
  });
  it("paused shows the resume time", () => {
    const at = new Date(2026, 9, 26, 17, 5).toISOString();
    const s = statusOf({ type: "paused-by-limit", resume_at: at });
    expect(s.tone).toBe("pause");
    expect(s.label).toBe("En pausa");
    expect(s.detail).toBe("se reanuda a las 17:05");
  });
});

const pr = (snapshot: Partial<NonNullable<PullRequest["snapshot"]>> | null): PullRequest =>
  ({ id: 1, number: 9, snapshot: snapshot && { state: "open", draft: false, ci: "success", ...snapshot } }) as PullRequest;

describe("prStateOf", () => {
  it("maps PR states", () => {
    expect(prStateOf(pr({ draft: true }))).toMatchObject({ state: "draft", label: "Borrador" });
    expect(prStateOf(pr({}))).toMatchObject({ state: "open", label: "Abierta" });
    expect(prStateOf(pr({ ci: "failure" }))).toMatchObject({ state: "failing", label: "Checks fallan" });
    expect(prStateOf(pr({ state: "merged", ci: "failure" }))).toMatchObject({ state: "merged", label: "Fusionada" });
    expect(prStateOf(pr(null))).toMatchObject({ state: "open" });
  });
});

describe("sessionDot", () => {
  const s = (state: Session["state"], live = true) => ({ state, live }) as Session;
  it("maps sessions to a dot", () => {
    expect(sessionDot(s("waiting"))).toBe("wait");
    expect(sessionDot(s("working"))).toBe("run");
    expect(sessionDot(s("starting"))).toBe("run");
    expect(sessionDot(s("paused"))).toBe("neutral");
    expect(sessionDot(s(null))).toBe("neutral");
    expect(sessionDot(s("working", false))).toBe("neutral");
  });
});

describe("kindLabel", () => {
  it("translates", async () => {
    const { kindLabel } = await import("./vocab");
    setLanguage("en");
    expect(kindLabel("Pregunta")).toBe("Question");
    setLanguage("es");
    expect(kindLabel("Aviso")).toBe("Aviso");
  });
});
