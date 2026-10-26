import { describe, expect, it } from "vitest";
import type { Action, Activity, Helper, Session } from "../api";
import { t } from "../i18n";
import type { DeskModel } from "./layout";
import { accessibleName, caption, deskState, floorState, roleLook } from "./layout";

const session = (id: string, task: string, over: Partial<Session> = {}): Session =>
  ({
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
  }) as Session;
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

const helper = (id: string, role: string, since = "2026-01-01T00:00:0" + id.slice(-1) + "Z", kind: Action["kind"] | null = null): Helper => ({
  id,
  role,
  since,
  action: kind && { id: "a" + id, kind, tool: "T", target: null, background: false, since },
});

describe("roleLook", () => {
  it("maps known roles to a look", () => {
    for (const r of ["code-researcher", "Explore", "doc-searcher"]) expect(roleLook(r).look).toBe("researcher");
    expect(roleLook("implementer").look).toBe("implementer");
    for (const r of ["code-quality-reviewer", "spec-compliance-reviewer"]) expect(roleLook(r).look).toBe("reviewer");
  });
  it("gives other roles the generic look labelled with the role", () => {
    expect(roleLook("foo")).toEqual({ look: "generic", label: "foo" });
    expect(roleLook("")).toEqual({ look: "generic", label: t("Ayudante") });
  });
});

describe("accessibleName with helpers", () => {
  it("appends the helpers and what they do", () => {
    const desk = { session: { state: "working" } as Session, owner: "K", key: "K", title: "", phase: null, project: null, slot: 0 } as DeskModel;
    const activity: Activity = {
      action: null,
      helpers: [helper("h1", "code-researcher", undefined, "read"), helper("h2", "implementer", undefined, "edit")],
    };
    expect(accessibleName(desk, activity)).toBe(
      `K, ${t("Pensando")}, ${t("{n} ayudantes", { n: 2 })}: code-researcher ${t("Leyendo").toLowerCase()}, implementer ${t("Editando").toLowerCase()}`,
    );
  });
});

describe("floorState", () => {
  it("maps the session state to the office table", () => {
    expect(floorState(session("a", "T-1", { state: "starting" }))).toBe("starting");
    expect(floorState(session("a", "T-1", { state: "waiting" }))).toBe("waiting");
    expect(floorState(session("a", "T-1", { state: "paused" }))).toBe("paused");
    expect(floorState(session("a", "T-1", { state: "working" }))).toBe("working");
  });
});

describe("accessibleName with tokens", () => {
  const desk = { session: session("a", "T-1"), owner: "T-1", key: "T-1", title: "x", phase: null, project: null, slot: 0 } as DeskModel;

  it("appends the formatted tokens when given", () => {
    expect(accessibleName(desk, undefined, 1_200_000)).toBe(accessibleName(desk) + ", 1.2M tokens");
  });

  it("adds nothing without tokens", () => {
    expect(accessibleName(desk, undefined, 0)).toBe(accessibleName(desk));
  });
});
