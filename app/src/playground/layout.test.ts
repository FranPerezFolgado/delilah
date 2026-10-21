import { describe, expect, it } from "vitest";
import type { Action, Activity, Project, Session, SpecSummary, Task } from "../api";
import { t } from "../i18n";
import type { DeskModel } from "./layout";
import { accessibleName, assignSlots, caption, deskState, groupByProject, slotMaps } from "./layout";

describe("assignSlots", () => {
  it("numbers from 0 in input order when nothing was assigned", () => {
    expect([...assignSlots(new Map(), ["a", "b", "c"])]).toEqual([["a", 0], ["b", 1], ["c", 2]]);
  });

  it("keeps the slot of kept ids when others are added or removed", () => {
    const prev = new Map([["a", 0], ["b", 1], ["c", 2]]);
    const next = assignSlots(prev, ["c", "a", "d"]);
    expect(next.get("a")).toBe(0);
    expect(next.get("c")).toBe(2);
    expect(next.has("b")).toBe(false);
  });

  it("gives a new id the lowest free slot, a hole first", () => {
    const prev = new Map([["a", 0], ["c", 2]]);
    expect(assignSlots(prev, ["a", "c", "d"]).get("d")).toBe(1);
    expect(assignSlots(prev, ["a", "c", "d", "e"]).get("e")).toBe(3);
  });
});

const session = (id: string, task: string, over: Partial<Session> = {}): Session => ({
  id,
  task,
  kind: "claude",
  label: "claude",
  primary: true,
  closed: false,
  live: true,
  read_only: false,
  state: "working",
  ...over,
});
const task = (key: string, project: string | null, over: Partial<Task> = {}): Task => ({
  key,
  title: `title ${key}`,
  project,
  workflow: "feature",
  phase: null,
  status: { type: "running" },
  ...over,
});
const proj = (name: string): Project => ({ name, path: `/p/${name}`, kind: "repo", workflow: "feature" });

describe("groupByProject", () => {
  const projects = [proj("web"), proj("api"), proj("empty")];

  it("keeps only live claude sessions, areas in projects order, only projects with a desk", () => {
    const areas = groupByProject(
      [
        session("s1", "A-1"),
        session("s2", "W-1"),
        session("sh", "W-1", { kind: "shell" }),
        session("dead", "W-1", { live: false }),
      ],
      [task("A-1", "api"), task("W-1", "web")],
      projects,
      {},
    );
    expect(areas.map((a) => a.project)).toEqual(["web", "api"]);
    expect(areas[0].desks.map((d) => d.session.id)).toEqual(["s2"]);
  });

  it("describes a task desk with key, title and phase", () => {
    const specs = { "W-1": { dir: "specs/1", phase: "plan" } as unknown as SpecSummary };
    const [area] = groupByProject([session("s", "W-1")], [task("W-1", "web")], projects, specs);
    expect(area.desks[0]).toMatchObject({ owner: "W-1", key: "W-1", title: "title W-1", phase: "plan", project: "web" });
  });

  it("groups an @project session under the project, without phase", () => {
    const [area] = groupByProject([session("s", "@api")], [], projects, {});
    expect(area.project).toBe("api");
    expect(area.desks[0]).toMatchObject({ owner: "@api", key: "api", title: "api", phase: null, project: "api" });
  });

  it("puts sessions of tasks without project in a trailing area", () => {
    const areas = groupByProject([session("s1", "N-1"), session("s2", "W-1")], [task("N-1", null), task("W-1", "web")], projects, {});
    expect(areas.map((a) => a.project)).toEqual(["web", "—"]);
    expect(areas[1].desks[0].project).toBeNull();
  });
});

describe("deskState", () => {
  it("maps the session state, working without activity is thinking", () => {
    expect(deskState(session("s", "A", { state: "starting" }))).toBe("starting");
    expect(deskState(session("s", "A", { state: "waiting" }))).toBe("waiting");
    expect(deskState(session("s", "A", { state: "paused" }))).toBe("paused");
    expect(deskState(session("s", "A", { state: "working" }))).toBe("thinking");
  });

  const act = (kind: Action["kind"], background = false): Activity => ({
    action: { id: "a", kind, tool: "T", target: null, background, since: "2026-01-01T00:00:00Z" },
    helpers: [],
  });
  const working = session("s", "A", { state: "working" });
  it("maps the action kind to a state", () => {
    expect(deskState(working, act("edit"))).toBe("edit");
    expect(deskState(working, act("read"))).toBe("read");
    expect(deskState(working, act("run"))).toBe("run");
    expect(deskState(working, act("other"))).toBe("other");
    expect(deskState(working, { action: null, helpers: [] })).toBe("thinking");
  });
  it("supervises on a foreground delegation, thinks on a background one", () => {
    expect(deskState(working, act("delegate", false))).toBe("supervising");
    expect(deskState(working, act("delegate", true))).toBe("thinking");
  });
  it("ignores the activity unless working", () => {
    expect(deskState(session("s", "A", { state: "waiting" }), act("edit"))).toBe("waiting");
    expect(deskState(session("s", "A", { state: "paused" }), act("run"))).toBe("paused");
  });
});

describe("caption", () => {
  const a = (tool: string, target: string | null): Action => ({ id: "a", kind: "edit", tool, target, background: false, since: "" });
  it("joins tool and target, or gives the tool alone", () => {
    expect(caption(a("Edit", "api.rs"))).toBe("Edit · api.rs");
    expect(caption(a("Bash", "cargo test…"))).toBe("Bash · cargo test…");
    expect(caption(a("Task", null))).toBe("Task");
    expect(caption(null)).toBeNull();
    expect(caption(undefined)).toBeNull();
  });
});

describe("slots across regroupings", () => {
  const projects = [proj("web")];
  const tasks = [task("A", "web"), task("B", "web"), task("C", "web")];
  it("keeps the slot of a desk when another leaves and a new one arrives", () => {
    const first = groupByProject([session("a", "A"), session("b", "B")], tasks, projects, {});
    expect(first[0].desks.map((d) => d.slot)).toEqual([0, 1]);
    const second = groupByProject([session("b", "B"), session("c", "C")], tasks, projects, {}, slotMaps(first));
    expect(Object.fromEntries(second[0].desks.map((d) => [d.session.id, d.slot]))).toEqual({ b: 1, c: 0 });
  });
});

describe("accessibleName", () => {
  const model = (state: Session["state"], key = "LIBRE-3") =>
    ({ session: { state } as Session, owner: key, key, title: "", phase: null, project: null, slot: 0 }) as DeskModel;
  it("names the task and the state", () => {
    expect(accessibleName(model("waiting"))).toBe(`LIBRE-3, ${t("Esperándote")}`);
    expect(accessibleName(model("working"))).toBe(`LIBRE-3, ${t("Pensando")}`);
    expect(accessibleName(model("paused"))).toBe(`LIBRE-3, ${t("En pausa")}`);
    expect(accessibleName(model("starting"))).toBe(`LIBRE-3, ${t("Arrancando")}`);
  });
  it("includes the caption and the supervising word", () => {
    const act = (kind: Action["kind"], tool: string, target: string | null): Activity => ({
      action: { id: "a", kind, tool, target, background: false, since: "" },
      helpers: [],
    });
    expect(accessibleName(model("working"), act("edit", "Edit", "api.rs"))).toBe(`LIBRE-3, ${t("Editando")}, Edit · api.rs`);
    expect(accessibleName(model("working"), act("delegate", "Task", null))).toBe(`LIBRE-3, ${t("Supervisando")}, Task`);
  });
  it("uses the project name for an @project desk", () => {
    expect(accessibleName(model("working", "web"))).toBe(`web, ${t("Pensando")}`);
  });
});
