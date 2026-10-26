// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, within } from "@testing-library/react";
import { DEFAULT_PROJECT_SETTINGS, type Project, type Session, type Task, type TaskStatus } from "../api";
import { Sidebar } from "./Sidebar";
import { setLanguage } from "../i18n";

const web: Project = {
  name: "web",
  path: "/w/web",
  kind: "repo",
  workflow: "feature",
  settings: DEFAULT_PROJECT_SETTINGS,
};
const task = (key: string, status: TaskStatus): Task => ({
  key,
  title: key,
  project: "web",
  workflow: "feature",
  phase: null,
  saving_mode: false, title_source: "initial", previous_title: null, branch_error: null,
  handoff: null,
  status,
});

type Extra = Partial<React.ComponentProps<typeof Sidebar>>;

function setup(tasks: Task[], sessions: Session[] = [], extra: Extra = {}) {
  const onDelete = vi.fn();
  const onIdea = vi.fn();
  const onOpenIdeas = vi.fn();
  const onView = vi.fn();
  const onToggleRail = vi.fn();
  const onSelect = vi.fn();
  const onDone = vi.fn();
  const view = render(
    <Sidebar
      view="tasks"
      onView={onView}
      tasks={tasks}
      projects={[web]}
      sessions={sessions}
      selected={null}
      onSelect={onSelect}
      onOpenSession={() => {}}
      onAsk={() => {}}
      onIdea={onIdea}
      onOpenIdeas={onOpenIdeas}
      onDelete={onDelete}
      onDone={onDone}
      onToggleRail={onToggleRail}
      waiting={0}
      {...extra}
    />,
  );
  const keys = () => [...view.container.querySelectorAll(".sb-taskrow .sb-key")].map((k) => k.textContent);
  return { ...view, keys, onDelete, onIdea, onOpenIdeas, onView, onToggleRail, onSelect, onDone };
}

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  localStorage.clear();
  setLanguage("en");
});

describe("Sidebar nav", () => {
  it("lists Bandeja, Tareas, Jira and Proyectos with shortcuts only in tooltips", () => {
    const { getByRole, container } = setup([task("PROJ-1", { type: "running" })], [], { waiting: 2 });
    for (const name of [/^Bandeja/, /^Tareas/, /^Jira/, /^Proyectos/]) expect(getByRole("button", { name })).toBeTruthy();
    expect(container.querySelector(".kbd")).toBeNull();
    expect(container.querySelector('[title^="Bandeja ("]')).not.toBeNull();
  });

  it("marks the inbox count as hot and the task count as neutral", () => {
    const { getByRole } = setup([task("PROJ-1", { type: "running" })], [], { waiting: 2 });
    expect(getByRole("button", { name: /^Bandeja/ }).querySelector(".cnt.hot")!.textContent).toBe("2");
    const tasks = getByRole("button", { name: /^Tareas/ }).querySelector(".cnt")!;
    expect(tasks.textContent).toBe("1");
    expect(tasks.classList.contains("hot")).toBe(false);
  });

  it("has no + Tarea button; adding a project is the + next to Proyectos", () => {
    const onAddProject = vi.fn();
    const { getByRole, queryByRole } = setup([], [], { onAddProject });
    expect(queryByRole("button", { name: /^Tarea\b(?!s)/ })).toBeNull();
    fireEvent.click(getByRole("button", { name: "Añadir proyecto" }));
    expect(onAddProject).toHaveBeenCalled();
  });

  it("falls back to the Proyectos view for Añadir proyecto", () => {
    const { getByRole, onView } = setup([]);
    fireEvent.click(getByRole("button", { name: "Añadir proyecto" }));
    expect(onView).toHaveBeenCalledWith("projects");
  });

  it("collapses with the Plegar barra lateral button, shortcut in its tooltip", () => {
    const { getByRole, onToggleRail, container } = setup([]);
    fireEvent.click(getByRole("button", { name: "Plegar barra lateral" }));
    expect(onToggleRail).toHaveBeenCalled();
    expect(container.querySelector('[title^="Plegar barra lateral ("]')).not.toBeNull();
    expect(getByRole("button", { name: /^Ajustes/ })).toBeTruthy();
  });
});

describe("Sidebar project and task rows", () => {
  it("names the project button by the project, not its path", () => {
    const { getByRole } = setup([]);
    expect(getByRole("button", { name: "web" })).toBeTruthy();
  });

  it("puts the title first and the key in the row, with a tooltip of key, title and status", () => {
    const long = { ...task("PROJ-9", { type: "running" }), title: "A very long title that must end in an ellipsis in the sidebar" };
    const { container } = setup([long]);
    const row = container.querySelector(".sb-taskrow")!;
    expect(row.querySelector(".sb-title")!.textContent).toBe(long.title);
    expect(row.querySelector(".sb-key")!.textContent).toBe("PROJ-9");
    expect(row.querySelector(".sb-title")!.getAttribute("title")).toBe(`PROJ-9 · ${long.title} · Trabajando`);
    expect(row.querySelector(".sb-dot.run")).not.toBeNull();
  });

  it("marks the selected task row as current", () => {
    const { container } = setup([task("PROJ-1", { type: "running" })], [], { selected: "PROJ-1" });
    expect(container.querySelector(".sb-taskrow")!.getAttribute("aria-current")).toBe("true");
  });

  it("selects a task and a project", () => {
    const { container, getByRole, onSelect } = setup([task("PROJ-1", { type: "running" })]);
    fireEvent.click(container.querySelector(".sb-taskrow")!);
    expect(onSelect).toHaveBeenCalledWith("PROJ-1");
    fireEvent.click(getByRole("button", { name: "web" }));
    expect(onSelect).toHaveBeenCalledWith("@web");
  });

  it("counts done tasks as «N hechas»", () => {
    const { container } = setup([task("PROJ-1", { type: "done" }), task("PROJ-2", { type: "done" })]);
    expect(container.querySelector(".sb-done")!.textContent).toBe("2 hechas");
  });

  it("groups tasks without a project under «Sin proyecto»", () => {
    const orphan = { ...task("PROJ-5", { type: "running" }), project: null };
    const { getByRole } = setup([orphan]);
    expect(getByRole("button", { name: "Sin proyecto" })).toBeTruthy();
  });

  it("asks Claude about the project from its hover action", () => {
    const onAsk = vi.fn();
    const { getByLabelText } = setup([], [], { onAsk });
    fireEvent.click(getByLabelText("Preguntar a Claude sobre web"));
    expect(onAsk).toHaveBeenCalledWith("web");
  });
});

describe("Sidebar backlog", () => {
  it("keeps backlog tasks folded apart, oldest first, after the work in progress", () => {
    const { getByRole, keys } = setup([
      task("PROJ-1", { type: "queued" }),
      task("PROJ-2", { type: "running" }),
      task("PROJ-3", { type: "queued" }),
      task("PROJ-4", { type: "waiting-for-user" }),
    ]);
    // Urgent first; the backlog is not pending work.
    expect(keys()).toEqual(["PROJ-4", "PROJ-2"]);
    const head = getByRole("button", { name: "Backlog · 2" });
    expect(head.getAttribute("aria-expanded")).toBe("false");

    fireEvent.click(head);
    expect(head.getAttribute("aria-expanded")).toBe("true");
    expect(keys()).toEqual(["PROJ-4", "PROJ-2", "PROJ-1", "PROJ-3"]);
  });

  it("shows no backlog section without backlog tasks", () => {
    const { queryByRole } = setup([task("PROJ-2", { type: "running" })]);
    expect(queryByRole("button", { name: /^Backlog/ })).toBeNull();
  });
});

describe("Sidebar task menu", () => {
  const open = (tasks: Task[], extra: Extra = {}) => {
    const s = setup(tasks, [], extra);
    fireEvent.contextMenu(s.container.querySelector(".sb-taskrow")!);
    return s;
  };

  it("offers Abrir la tarea, Marcar como hecha and Eliminar tarea…", () => {
    const { getAllByRole } = open([task("PROJ-2", { type: "running" })]);
    expect(getAllByRole("menuitem").map((m) => m.textContent)).toEqual(["Abrir la tarea", "Marcar como hecha", "Eliminar tarea…"]);
  });

  it("opens the task and marks it done", () => {
    const tasks = [task("PROJ-2", { type: "running" })];
    const { getByRole, onSelect } = open(tasks);
    fireEvent.click(getByRole("menuitem", { name: "Abrir la tarea" }));
    expect(onSelect).toHaveBeenCalledWith("PROJ-2");
    const second = open(tasks, {});
    fireEvent.click(within(second.container).getAllByRole("menuitem", { name: "Marcar como hecha" })[0]);
    expect(second.onDone).toHaveBeenCalledWith(tasks[0]);
  });

  it("deletes only after the confirmation dialog", () => {
    const tasks = [task("PROJ-2", { type: "running" })];
    const { getByRole, onDelete } = open(tasks);
    fireEvent.click(getByRole("menuitem", { name: "Eliminar tarea…" }));
    expect(onDelete).not.toHaveBeenCalled();
    const dialog = getByRole("dialog");
    expect(dialog.textContent).toContain("PROJ-2");
    fireEvent.click(within(dialog).getByRole("button", { name: "Eliminar tarea" }));
    expect(onDelete).toHaveBeenCalledWith(tasks[0]);
  });

  it("cancels the deletion", () => {
    const { getByRole, onDelete } = open([task("PROJ-2", { type: "running" })]);
    fireEvent.click(getByRole("menuitem", { name: "Eliminar tarea…" }));
    fireEvent.click(within(getByRole("dialog")).getByRole("button", { name: "Cancelar" }));
    expect(onDelete).not.toHaveBeenCalled();
  });
});

describe("Sidebar rail", () => {
  it("renders the 56 px rail with icons named by their label and the project initial", () => {
    const { container, getByRole, onToggleRail, onSelect } = setup([task("PROJ-1", { type: "waiting-for-user" })], [], { rail: true, waiting: 1 });
    expect(container.querySelector("nav.sidebar.rail")).not.toBeNull();
    for (const name of ["Bandeja", "Tareas", "Jira", "Proyectos", "Ajustes"]) expect(getByRole("button", { name })).toBeTruthy();
    const initial = getByRole("button", { name: "web" });
    expect(initial.textContent).toBe("W");
    expect(initial.querySelector(".sb-hot")).not.toBeNull();
    fireEvent.click(initial);
    expect(onSelect).toHaveBeenCalledWith("@web");
    expect(container.querySelector(".sb-taskrow")).toBeNull();
    fireEvent.click(getByRole("button", { name: "Desplegar barra lateral" }));
    expect(onToggleRail).toHaveBeenCalled();
  });
});

describe("Sidebar ideas", () => {
  it("starts an idea conversation from the project's «Nueva tarea en este proyecto» button", () => {
    const { getByLabelText, onIdea } = setup([]);
    fireEvent.click(getByLabelText("Nueva tarea en web"));
    expect(onIdea).toHaveBeenCalledWith("web");
  });

  const idea = (id: string, state = "waiting") => ({ id, task: "@web", kind: "claude", label: "Idea " + id, primary: false, closed: false, live: true, read_only: false, state, conversation: true }) as unknown as Session;
  const ro = { id: "r1", task: "@web", kind: "claude", label: "Ask", primary: false, closed: false, live: true, read_only: true, state: "idle", conversation: false } as unknown as Session;
  const rows = (c: HTMLElement) => [...c.querySelectorAll(".sb-row:not(.sb-fold)")];

  it("shows no Ideas row without idea conversations", () => {
    const { container } = setup([], [ro]);
    expect(container.querySelector(".ideas")).toBeNull();
  });

  it("shows one Ideas · N row for the ideas", () => {
    const { container } = setup([], [idea("a")]);
    expect(container.querySelector(".ideas")!.textContent).toContain("Ideas · 1");
  });

  it("collapses many ideas into one first row, leaving other sessions alone", () => {
    const { container } = setup([], [ro, idea("a"), idea("b"), idea("c")]);
    const r = rows(container);
    expect(r[0].classList.contains("ideas")).toBe(true);
    expect(r[0].textContent).toContain("Ideas · 3");
    expect(r.length).toBe(2);
    expect(r[1].textContent).toContain("Ask");
  });

  it("opens the project's ideas, only the ideas and in list order", () => {
    const { getByRole, onOpenIdeas } = setup([], [idea("a"), ro, idea("b"), idea("c")]);
    fireEvent.click(getByRole("button", { name: "Ideas de web (3)" }));
    expect(onOpenIdeas).toHaveBeenCalledTimes(1);
    expect(onOpenIdeas).toHaveBeenCalledWith("@web", [idea("a"), idea("b"), idea("c")]);
  });

  it("shows the idea icon on the row", () => {
    const { container } = setup([], [idea("a")]);
    expect(container.querySelector(".ideas svg.idea")).not.toBeNull();
  });

  it("puts a dot on the left of conversation rows", () => {
    const live = { ...ro, read_only: false, state: "waiting" } as unknown as Session;
    const { container } = setup([], [live]);
    expect(container.querySelector(".sb-row .row-lead .sb-dot.wait")).not.toBeNull();
  });

  it("orders the Ideas row, then other sessions, then tasks", () => {
    const { container } = setup([task("PROJ-1", { type: "running" })], [ro, idea("a")]);
    const r = rows(container);
    expect(r.length).toBe(3);
    expect(r[0].classList.contains("ideas")).toBe(true);
    expect(r[1].textContent).toContain("Ask");
    expect(r[2].querySelector(".sb-key")!.textContent).toBe("PROJ-1");
  });

  it("hides the rows with the folded project, whose count still includes ideas and is hot", () => {
    const { container, getByLabelText } = setup([task("PROJ-1", { type: "running" })], [ro, idea("a"), idea("b")]);
    fireEvent.click(getByLabelText("Plegar web"));
    expect(container.querySelector(".ideas")).toBeNull();
    const count = container.querySelector(".sb-head .sb-count")!;
    expect(count.textContent).toBe("4");
    expect(count.classList.contains("hot")).toBe(true);
  });
});
