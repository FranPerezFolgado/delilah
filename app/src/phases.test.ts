import { describe, expect, it } from "vitest";
import type { Task, TaskStatus } from "./api";
import { boardColumnOf } from "./phases";
import { whereOf } from "./components/WherePicker";

const task = (status: TaskStatus, phase: string | null = null): Task => ({
  key: "PROJ-1",
  title: "x",
  project: "web",
  workflow: "feature",
  phase,
  status,
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

describe("whereOf", () => {
  const mode = (workspace: "checkout" | "worktree") => ({ settings: { workspace } }) as never;
  it("a saved choice wins; without one, the project's default (checkout if none), as the daemon does", () => {
    expect(whereOf(true)).toBe("checkout");
    expect(whereOf(false)).toBe("worktree");
    expect(whereOf(null)).toBe("checkout");
    expect(whereOf(undefined)).toBe("checkout");
    expect(whereOf(null, mode("worktree"))).toBe("worktree");
    expect(whereOf(null, mode("checkout"))).toBe("checkout");
    expect(whereOf(true, mode("worktree"))).toBe("checkout");
  });
});
