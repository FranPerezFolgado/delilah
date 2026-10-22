import { describe, expect, it } from "vitest";
import type { Autopilot, Handoff, Session, Step, Task } from "./api";
import { chainOf } from "./chain";

const task = (saving: boolean, handoff: Handoff | null = null): Task => ({
  key: "PROJ-1",
  title: "T",
  project: "web",
  workflow: "feature",
  phase: null,
  saving_mode: saving,
  handoff,
  status: { type: "running" },
});
const sess = (id: string, step: Step | null, primary: boolean): Session => ({
  id,
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
