import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { Autopilot, Handoff, Session, Step, Task } from "./api";
import { chainOf, sessionName } from "./chain";
import { setLanguage } from "./i18n";

const task = (saving: boolean, handoff: Handoff | null = null): Task => ({
  created_at: "2026-01-01T00:00:00Z", finished_at: null, key: "PROJ-1",
  title: "T",
  project: "web",
  workflow: "feature",
  phase: null,
  saving_mode: saving, title_source: "initial", previous_title: null, branch_error: null,
  handoff,
  status: { type: "running" },
});
const sess = (id: string, step: Step | null, primary: boolean): Session => ({
  effective_runtime: "pty", runtime_override: null, last_activity_at: null, last_message: null, id,
  task: "PROJ-1",
  kind: "claude",
  label: id,
  primary,
  closed: false,
  live: true,
  read_only: false,
  conversation: false,
  state: "working",
  step,
});

describe("chainOf", () => {
  it("is null without saving mode", () => {
    expect(chainOf(task(false), [])).toBeNull();
  });

  it("lists the three steps in order", () => {
    const c = chainOf(task(true), [sess("a", "plan", true)])!;
    expect(c.map((x) => x.step)).toEqual(["plan", "implement", "review"]);
  });

  it("marks the primary's step active and the rest pending", () => {
    const c = chainOf(task(true), [sess("a", "plan", true)])!;
    expect(c.map((x) => x.state)).toEqual(["active", "pending", "pending"]);
    expect(c[0].session?.id).toBe("a");
  });

  it("marks earlier steps with a session done", () => {
    const c = chainOf(task(true), [sess("a", "plan", false), sess("b", "implement", true)])!;
    expect(c.map((x) => x.state)).toEqual(["done", "active", "pending"]);
  });

  it.each(["offered", "saving", "starting"] as const)("waits on the next step while %s", (stage) => {
    const c = chainOf(task(true, { to: "implement", stage }), [sess("a", "plan", true)])!;
    expect(c.map((x) => x.state)).toEqual(["active", "waiting", "pending"]);
  });

  it("waits with the error when failed", () => {
    const c = chainOf(task(true, { to: "review", stage: "failed", error: "boom" }), [
      sess("a", "plan", false),
      sess("b", "implement", true),
    ])!;
    expect(c[2]).toMatchObject({ state: "waiting", error: "boom" });
  });

  it("keeps the plan step active when implement was skipped", () => {
    const c = chainOf(task(true, { to: "implement", stage: "skipped" }), [sess("a", "plan", true)])!;
    expect(c.map((x) => x.state)).toEqual(["active", "pending", "pending"]);
  });

  describe("with autopilot", () => {
    const ap = (a: Partial<Autopilot> = {}): Task => ({
      ...task(true),
      autopilot: { state: "running", since: "2026-10-21T10:00:00Z", ...a },
    });
    const all = [sess("a", "plan", false), sess("b", "implement", false), sess("c", "review", true)];

    it("adds preview and PR steps only with a preview command", () => {
      expect(chainOf(ap(), all, { hasPreview: true })!.map((x) => x.step)).toEqual(["plan", "implement", "review", "preview", "pr"]);
      expect(chainOf(ap(), all, { hasPreview: false })!.map((x) => x.step)).toEqual(["plan", "implement", "review", "pr"]);
      expect(chainOf(ap(), all)!.map((x) => x.step)).toEqual(["plan", "implement", "review", "pr"]);
    });

    it("has no extra steps without autopilot", () => {
      expect(chainOf(task(true), all, { hasPreview: true })!).toHaveLength(3);
    });

    it("marks preview active at its stage and PR active when opening", () => {
      const p = chainOf(ap({ stage: "preview" }), all, { hasPreview: true })!;
      expect(p.map((x) => x.state)).toEqual(["done", "done", "done", "active", "pending"]);
      const o = chainOf(ap({ stage: "opening_pr" }), all, { hasPreview: true })!;
      expect(o.map((x) => x.state).slice(3)).toEqual(["done", "active"]);
    });

    it("marks PR done once linked or when autopilot is done", () => {
      expect(chainOf(ap(), all, { prLinked: true })!.at(-1)!.state).toBe("done");
      expect(chainOf(ap({ state: "done" }), all)!.at(-1)!.state).toBe("done");
    });

    it("shows the current step waiting with the reason when paused", () => {
      const c = chainOf(ap({ state: "paused", stage: "preview", reason: "preview failed" }), all, { hasPreview: true })!;
      expect(c[3]).toMatchObject({ step: "preview", state: "waiting", error: "preview failed" });
      const d = chainOf(ap({ state: "paused", reason: "no consigo avanzar" }), all)!;
      expect(d.find((x) => x.state === "waiting")).toMatchObject({ step: "review", error: "no consigo avanzar" });
    });

    it("marks the last unfinished step waiting when paused with nothing active", () => {
      const none = [sess("a", "plan", false), sess("b", "implement", false), sess("c", "review", false)];
      const c = chainOf(ap({ state: "paused", reason: "parado" }), none)!;
      expect(c.filter((x) => x.state === "waiting")).toEqual([expect.objectContaining({ step: "pr", error: "parado" })]);
    });
  });
});

describe("sessionName", () => {
  beforeEach(() => setLanguage("es"));
  afterEach(() => setLanguage("es"));
  const named = (o: Partial<Session>): Session => ({ ...sess("Otra cosa", null, false), ...o });

  it("names a session after its step, whatever primary and live are", () => {
    for (const primary of [true, false]) {
      for (const live of [true, false]) {
        expect(sessionName(named({ step: "plan", primary, live }))).toBe("Spec y plan");
        expect(sessionName(named({ step: "implement", primary, live }))).toBe("Implementación");
        expect(sessionName(named({ step: "review", primary, live }))).toBe("Revisión");
      }
    }
  });

  it("follows the UI language", () => {
    setLanguage("en");
    expect(sessionName(named({ step: "plan" }))).toBe("Spec and plan");
    expect(sessionName(named({ step: "implement" }))).toBe("Implement");
    expect(sessionName(named({ step: "review" }))).toBe("Review");
    expect(sessionName(named({ primary: true }))).toBe("Main Claude");
  });

  it("calls the primary claude without a step Claude principal", () => {
    expect(sessionName(named({ primary: true }))).toBe("Claude principal");
  });

  it("keeps the label of a primary shell and of non-primary sessions without a step", () => {
    expect(sessionName(named({ primary: true, kind: "shell" }))).toBe("Otra cosa");
    expect(sessionName(named({ primary: false }))).toBe("Otra cosa");
  });
});
