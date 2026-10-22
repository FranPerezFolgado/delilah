import { describe, expect, it } from "vitest";
import type { Handoff, Session, Step, Task } from "./api";
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
});
