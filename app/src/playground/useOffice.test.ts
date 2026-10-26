import { describe, expect, it } from "vitest";
import type { Activity, Helper, Project, Session, Task } from "../api";
import { buildFloors, emptyFloor } from "./useOffice";

const session = (id: string, task: string, over: Partial<Session> = {}): Session =>
  ({ id, task, kind: "claude", label: "claude", primary: true, closed: false, live: true, read_only: false, state: "working", ...over }) as Session;
const task = (key: string, project: string | null): Task =>
  ({ key, title: `title ${key}`, project, workflow: "feature", phase: null, status: { type: "running" } }) as Task;
const helper = (id: string, role = "implementer"): Helper => ({ id, role, action: null, since: "2026-01-01T00:00:00Z" });
const act = (...helpers: Helper[]): Activity => ({ action: null, helpers });
const base = { tasks: [task("T-1", "web"), task("T-2", "web")], projects: [] as Project[], specs: {}, tokens: {} };

describe("buildFloors", () => {
  it("groups live Claude sessions by owner, leads first", () => {
    const floors = buildFloors(
      { ...base, sessions: [session("a", "T-1"), session("b", "T-2"), session("c", "T-1"), session("sh", "T-1", { kind: "shell" }), session("p", "@web")], activity: {} },
      new Map(),
      0,
    );
    expect(floors.map((f) => f.owner)).toEqual(["T-1", "T-2", "@web"]);
    expect(floors[0].leads.map((l) => l.session)).toEqual(["a", "c"]);
    expect(floors[0].leads.map((l) => l.slot)).toEqual([0, 1]);
    expect(floors[0].title).toBe("title T-1");
    expect(floors[2].title).toBe("web");
    expect(floors[2].project).toBe("web");
  });

  it("seats helpers after the leads and keeps slots when a state changes", () => {
    const input = { ...base, sessions: [session("a", "T-1")], activity: { a: act(helper("h1"), helper("h2")) } };
    const first = buildFloors(input, new Map(), 0);
    expect(first[0].helpers.map((h) => h.slot)).toEqual([1, 2]);
    const prev = new Map(first.map((f) => [f.owner, f]));
    const second = buildFloors({ ...input, sessions: [session("a", "T-1", { state: "waiting" })] }, prev, 10);
    expect(second[0].leads[0]).toMatchObject({ slot: 0, state: "waiting" });
    expect(second[0].helpers.map((h) => h.slot)).toEqual([1, 2]);
    expect(second[0].waiting).toBe(1);
  });

  it("a finished helper leaves for 5 s, its slot is free at once", () => {
    const input = { ...base, sessions: [session("a", "T-1")], activity: { a: act(helper("h1")) } };
    const first = buildFloors(input, new Map(), 0);
    const prev1 = new Map(first.map((f) => [f.owner, f]));
    const second = buildFloors({ ...input, activity: { a: act() } }, prev1, 1000);
    expect(second[0].helpers).toEqual([]);
    expect(second[0].leaving).toEqual([{ id: "h1", slot: 1, since: 1000 }]);
    const prev2 = new Map(second.map((f) => [f.owner, f]));
    const third = buildFloors({ ...input, activity: { a: act(helper("h3")) } }, prev2, 2000);
    expect(third[0].helpers[0].slot).toBe(1);
    expect(third[0].leaving).toHaveLength(1);
    const gone = buildFloors({ ...input, activity: { a: act() } }, new Map(third.map((f) => [f.owner, f])), 6000);
    expect(gone[0].leaving.map((l) => l.id)).toEqual(["h3"]);
    const later = buildFloors({ ...input, activity: { a: act() } }, new Map(gone.map((f) => [f.owner, f])), 11000);
    expect(later[0].leaving).toEqual([]);
  });

  it("puts tokens on the lead and grows past four desks", () => {
    const hs = [1, 2, 3, 4, 5].map((n) => helper(`h${n}`));
    const floors = buildFloors({ ...base, sessions: [session("a", "T-1")], activity: { a: act(...hs) }, tokens: { a: { total: 1200 } as never } }, new Map(), 0);
    expect(floors[0].leads[0].tokens).toBe(1200);
    expect(Math.max(...floors[0].helpers.map((h) => h.slot))).toBe(5);
  });
});

describe("emptyFloor", () => {
  it("has the owner's title and nobody working", () => {
    const f = emptyFloor("T-2", base.tasks, {});
    expect(f).toMatchObject({ owner: "T-2", title: "title T-2", project: "web", leads: [], helpers: [], leaving: [], waiting: 0 });
    expect(emptyFloor("@web", [], {}).title).toBe("web");
  });
});
