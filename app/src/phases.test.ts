import { describe, expect, it } from "vitest";
import type { Task, TaskStatus } from "./api";
import { boardColumnOf, headerPhaseOf } from "./phases";
import { whereOf } from "./components/WherePicker";

const task = (status: TaskStatus, phase: string | null = null): Task => ({
  key: "PROJ-1",
  title: "x",
  project: "web",
  workflow: "feature",
  phase,
  status,
  saving_mode: false, title_source: "initial", previous_title: null, branch_error: null,
  handoff: null,
});

describe("boardColumnOf", () => {
  it("keeps backlog tasks in the backlog, whatever spec or phase they have", () => {
    const spec = { phase: "implement" as const, dir: "specs/001", done: 1, total: 3, current: null };
    expect(boardColumnOf(task({ type: "queued" }, "plan"), spec)).toBe("backlog");
  });

  it("puts started tasks in their phase", () => {
    expect(boardColumnOf(task({ type: "running" }, "plan"))).toBe("plan");
    expect(boardColumnOf(task({ type: "done" }))).toBe("done");
  });
});

describe("headerPhaseOf", () => {
  const spec = (phase: "specify" | "plan" | "implement" | "review", dir: string | null = "specs/001") => ({
    phase,
    dir,
    done: 0,
    total: 0,
    current: null,
  });

  it("shows nothing for backlog, finished tasks or tasks without a spec", () => {
    expect(headerPhaseOf(task({ type: "queued" }, "plan"), spec("plan"))).toBeNull();
    expect(headerPhaseOf(task({ type: "done" }, "plan"), spec("plan"))).toBeNull();
    expect(headerPhaseOf(task({ type: "running" }, "plan"), undefined)).toBeNull();
  });

  it("uses the recorded phase and its step", () => {
    expect(headerPhaseOf(task({ type: "running" }, "plan"), spec("implement"))).toEqual({ column: "plan", step: 2 });
  });

  it("falls back to the spec's phase, then to specify", () => {
    expect(headerPhaseOf(task({ type: "running" }), spec("implement"))).toEqual({ column: "implement", step: 3 });
    expect(headerPhaseOf(task({ type: "running" }), spec("review", null))).toEqual({ column: "specify", step: 1 });
  });

  it("keeps the recorded phase when the spec folder is missing", () => {
    expect(headerPhaseOf(task({ type: "running" }, "plan"), spec("review", null))).toEqual({ column: "plan", step: 2 });
  });

  it("agrees with the board's column", () => {
    const cases: [Task, ReturnType<typeof spec>][] = [
      [task({ type: "running" }, "plan"), spec("implement")],
      [task({ type: "running" }), spec("review")],
      [task({ type: "running" }), spec("review", null)],
      [task({ type: "running" }, "plan"), spec("review", null)],
    ];
    for (const [t, s] of cases) expect(headerPhaseOf(t, s)?.column).toBe(boardColumnOf(t, s));
  });

  it("counts done items out of total only in implement with a task list", () => {
    const running = task({ type: "running" }, "implement");
    expect(headerPhaseOf(running, { ...spec("implement"), done: 4, total: 12 })).toEqual({
      column: "implement",
      step: 3,
      done: 4,
      total: 12,
    });
    expect(headerPhaseOf(running, { ...spec("implement"), done: 0, total: 0 })).toEqual({ column: "implement", step: 3 });
    expect(headerPhaseOf(task({ type: "running" }, "plan"), { ...spec("plan"), total: 12 })).toEqual({
      column: "plan",
      step: 2,
    });
  });

  it("shows the phase for failed and paused tasks", () => {
    expect(headerPhaseOf(task({ type: "failed" } as TaskStatus, "plan"), spec("plan"))?.column).toBe("plan");
    expect(headerPhaseOf(task({ type: "paused-by-limit" } as TaskStatus, "plan"), spec("plan"))?.column).toBe("plan");
  });
});

describe("whereOf", () => {
  const mode = (workspace: "checkout" | "worktree") => ({ settings: { workspace } }) as never;
  it("a saved choice wins; without one, the project's setting; without that, the checkout, as the daemon does", () => {
    expect(whereOf(true)).toBe("checkout");
    expect(whereOf(false)).toBe("worktree");
    expect(whereOf(null)).toBe("checkout");
    expect(whereOf(undefined)).toBe("checkout");
    expect(whereOf(null, { settings: { workspace: null } } as never)).toBe("checkout");
    expect(whereOf(null, mode("worktree"))).toBe("worktree");
    expect(whereOf(null, mode("checkout"))).toBe("checkout");
    expect(whereOf(true, mode("worktree"))).toBe("checkout");
  });
  it("the new-task form passes its own fallback (checkout) for a project with no setting", () => {
    expect(whereOf(null, { settings: { workspace: null } } as never, "checkout")).toBe("checkout");
    expect(whereOf(null, mode("worktree"), "checkout")).toBe("worktree");
    expect(whereOf(false, undefined, "checkout")).toBe("worktree");
  });
});
