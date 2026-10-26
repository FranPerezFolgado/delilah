// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import type { Action, Activity, Helper, Session } from "../api";
import type { DeskModel, ProjectArea } from "./layout";
import { t } from "../i18n";
import { openPlaygroundWindow } from "../platform";
import { PlaygroundView } from "./PlaygroundView";
import { usePlayground } from "./usePlayground";

vi.mock("./usePlayground");

const desk = (id: string, slot: number, state: Session["state"], over: Partial<DeskModel> = {}): DeskModel => ({
  session: {
    id,
    task: id,
    kind: "claude",
    label: "claude",
    primary: true,
    closed: false,
    live: true,
    read_only: false,
    state,
  } as Session,
  owner: id,
  key: id,
  title: `title of ${id}`,
  phase: "implement",
  project: "web",
  slot,
  ...over,
});

function show(areas: ProjectArea[]) {
  vi.mocked(usePlayground).mockReturnValue({ areas, online: true });
}

vi.mock("../platform", () => ({ openPlaygroundWindow: vi.fn() }));

afterEach(cleanup);

describe("PlaygroundView window button", () => {
  it("opens the playground window; hidden when standalone", () => {
    show([]);
    const { getByText, queryByText, rerender } = render(<PlaygroundView onOpen={() => {}} />);
    fireEvent.click(getByText(t("Abrir en ventana")));
    expect(openPlaygroundWindow).toHaveBeenCalledTimes(1);
    rerender(<PlaygroundView onOpen={() => {}} standalone />);
    expect(queryByText(t("Abrir en ventana"))).toBeNull();
  });
});

describe("PlaygroundView", () => {
  it("renders an area per project with its desks", () => {
    show([
      { project: "web", desks: [desk("W-1", 0, "working")] },
      { project: "api", desks: [desk("A-1", 0, "waiting", { title: "x".repeat(80), phase: null })] },
    ]);
    const { container } = render(<PlaygroundView onOpen={() => {}} />);
    const sections = container.querySelectorAll("section");
    expect(sections).toHaveLength(2);
    expect([...sections].map((s) => s.querySelector("h3")?.textContent)).toEqual(["web", "api"]);
    expect(sections[0].textContent).toContain("W-1");
    expect(sections[0].textContent).toContain("title of W-1");
    expect(sections[1].querySelector("[title]")?.getAttribute("title")).toBe("x".repeat(80));
  });

  it("the character is a button whose pose is the desk state; click opens", () => {
    show([{ project: "web", desks: [desk("W-1", 0, "working"), desk("W-2", 1, "paused")] }]);
    const onOpen = vi.fn();
    const { container } = render(<PlaygroundView onOpen={onOpen} />);
    const buttons = [...container.querySelectorAll("button[data-pose]")];
    expect(buttons.map((b) => b.getAttribute("data-pose"))).toEqual(["thinking", "paused"]);
    fireEvent.click(buttons[0]);
    expect(onOpen).toHaveBeenCalledWith("W-1", "W-1");
  });

  it("shows the empty office without desks", () => {
    show([]);
    const { getByText } = render(<PlaygroundView onOpen={() => {}} />);
    expect(getByText(t("Ningún agente trabajando"))).toBeTruthy();
  });

  it("places each desk at its slot, and keeps it when others come and go", () => {
    show([{ project: "web", desks: [desk("A", 0, "working"), desk("B", 1, "working")] }]);
    const view = render(<PlaygroundView onOpen={() => {}} />);
    const slotOf = (id: string) => view.container.querySelector(`[data-owner="${id}"]`)?.getAttribute("data-slot");
    expect(slotOf("B")).toBe("1");
    show([{ project: "web", desks: [desk("B", 1, "working"), desk("C", 0, "working")] }]);
    view.rerender(<PlaygroundView onOpen={() => {}} />);
    expect(slotOf("B")).toBe("1");
    expect(slotOf("C")).toBe("0");
  });

  it("leaves a free slot as an empty cell so the desks keep their position", () => {
    show([{ project: "web", desks: [desk("A", 0, "working"), desk("C", 2, "working")] }]);
    const { container } = render(<PlaygroundView onOpen={() => {}} />);
    const grid = container.querySelector(".pg-grid")!;
    expect(grid.children).toHaveLength(3);
    expect(grid.children[2].getAttribute("data-owner")).toBe("C");
    expect(grid.children[1].getAttribute("aria-hidden")).toBe("true");
  });

  it("highlights a waiting desk and counts the waiting ones in every area", () => {
    show([
      { project: "web", desks: [desk("W-1", 0, "working"), desk("W-2", 1, "waiting")] },
      { project: "api", desks: [desk("A-1", 0, "working")] },
    ]);
    const { container, getByText } = render(<PlaygroundView onOpen={() => {}} />);
    const waiting = container.querySelectorAll(".pg-desk--waiting");
    expect(waiting).toHaveLength(1);
    expect(waiting[0].getAttribute("data-owner")).toBe("W-2");
    expect(getByText(t("{n} esperando", { n: 1 }))).toBeTruthy();
  });

  it("counts waiting desks across areas and drops to 0 when none", () => {
    show([
      { project: "web", desks: [desk("W-1", 0, "waiting")] },
      { project: "api", desks: [desk("A-1", 0, "waiting")] },
    ]);
    const view = render(<PlaygroundView onOpen={() => {}} />);
    expect(view.getByText(t("{n} esperando", { n: 2 }))).toBeTruthy();
    show([{ project: "web", desks: [desk("W-1", 0, "working")] }]);
    view.rerender(<PlaygroundView onOpen={() => {}} />);
    expect(view.getByText(t("{n} esperando", { n: 0 }))).toBeTruthy();
    expect(view.container.querySelector(".pg-desk--waiting")).toBeNull();
  });

  it("opens the session of a focused character on Enter (native button)", () => {
    show([{ project: "web", desks: [desk("@web", 0, "waiting", { owner: "@web", key: "web" })] }]);
    const onOpen = vi.fn();
    const { getByRole } = render(<PlaygroundView onOpen={onOpen} standalone />);
    const btn = getByRole("button") as HTMLButtonElement;
    btn.focus();
    expect(document.activeElement).toBe(btn);
    expect(btn.tagName).toBe("BUTTON");
    expect(btn.type).toBe("button");
    fireEvent.click(btn); // a focused button turns Enter into a click
    expect(onOpen).toHaveBeenCalledWith("@web", "@web");
  });

  it("gives each character its accessible name", () => {
    show([{ project: "web", desks: [desk("W-1", 0, "waiting")] }]);
    const { getByRole } = render(<PlaygroundView onOpen={() => {}} standalone />);
    expect(getByRole("button").getAttribute("aria-label")).toBe(`W-1, ${t("Esperándote")}`);
  });

  const act = (kind: Action["kind"], tool: string, target: string | null, background = false): Activity => ({
    action: { id: "a1", kind, tool, target, background, since: "2026-01-01T00:00:00Z" },
    helpers: [],
  });

  it("shows the action under an editing character, with the typing pose", () => {
    show([{ project: "web", desks: [desk("W-1", 0, "working", { activity: act("edit", "Edit", "api.rs") })] }]);
    const { container, getByRole } = render(<PlaygroundView onOpen={() => {}} standalone />);
    expect(container.querySelector("button[data-pose]")?.getAttribute("data-pose")).toBe("typing");
    expect(container.querySelector(".pg-caption")?.textContent).toBe("Edit · api.rs");
    expect(getByRole("button").getAttribute("aria-label")).toBe(`W-1, ${t("Editando")}, Edit · api.rs`);
  });

  it("a foreground delegate shows the phone and Supervisando", () => {
    show([{ project: "web", desks: [desk("W-1", 0, "working", { activity: act("delegate", "Task", "review") })] }]);
    const { container } = render(<PlaygroundView onOpen={() => {}} />);
    expect(container.querySelector("button[data-pose]")?.getAttribute("data-pose")).toBe("phone");
    expect(container.querySelector(".pg-caption")?.textContent).toBe(t("Supervisando"));
  });

  it("clears the caption without an action and on waiting desks", () => {
    show([
      {
        project: "web",
        desks: [desk("W-1", 0, "working"), desk("W-2", 1, "waiting", { activity: act("edit", "Edit", "api.rs") })],
      },
    ]);
    const { container } = render(<PlaygroundView onOpen={() => {}} />);
    expect(container.querySelector(".pg-caption")).toBeNull();
  });
});

describe("helpers around a desk", () => {
  const h = (id: string, role: string, n: number, kind: Action["kind"] | null = null): Helper => ({
    id,
    role,
    since: `2026-01-01T00:00:0${n}Z`,
    action: kind && { id: "a" + id, kind, tool: "T", target: null, background: false, since: "" },
  });
  const withHelpers = (helpers: Helper[]) =>
    show([{ project: "web", desks: [desk("W-1", 0, "working", { activity: { action: null, helpers } })] }]);
  const live = (c: HTMLElement) => [...c.querySelectorAll(".pg-helper:not(.pg-helper--leaving)")];

  it("renders each helper with look, pose and spot", () => {
    withHelpers([h("a", "code-researcher", 1, "read"), h("b", "implementer", 2)]);
    const { container } = render(<PlaygroundView onOpen={() => {}} />);
    const hs = live(container);
    expect(hs).toHaveLength(2);
    expect(hs.map((e) => e.getAttribute("data-look"))).toEqual(["researcher", "implementer"]);
    expect(hs.map((e) => e.getAttribute("data-pose"))).toEqual(["reading", "thinking"]);
    expect(hs.map((e) => e.getAttribute("data-spot"))).toEqual(["0", "1"]);
    expect(hs[0].getAttribute("role")).toBe("img");
  });

  it("shows 4 and a +2 badge titled with the other roles", () => {
    withHelpers(["r1", "r2", "r3", "r4", "r5", "r6"].map((r, i) => h(r, r, i)));
    const { container } = render(<PlaygroundView onOpen={() => {}} />);
    expect(live(container)).toHaveLength(4);
    const more = container.querySelector(".pg-helper-more")!;
    expect(more.textContent).toBe("+2");
    expect(more.getAttribute("title")).toBe("r5, r6");
  });

  it("keeps the spots of the remaining helpers when one is removed", () => {
    withHelpers([h("a", "x", 1), h("b", "y", 2), h("c", "z", 3)]);
    const view = render(<PlaygroundView onOpen={() => {}} />);
    withHelpers([h("a", "x", 1), h("c", "z", 3)]);
    view.rerender(<PlaygroundView onOpen={() => {}} />);
    const spots = live(view.container).map((e) => e.getAttribute("data-spot"));
    expect(spots).toEqual(["0", "2"]);
    expect(view.container.querySelectorAll(".pg-helper--leaving")).toHaveLength(1);
  });

  it("removes a leaving helper after 300 ms even if the desk re-renders meanwhile", () => {
    vi.useFakeTimers();
    try {
      withHelpers([h("a", "x", 1), h("b", "y", 2)]);
      const view = render(<PlaygroundView onOpen={() => {}} />);
      withHelpers([h("a", "x", 1)]);
      view.rerender(<PlaygroundView onOpen={() => {}} />);
      const out = view.container.querySelector(".pg-helper--leaving")!;
      expect(out.getAttribute("aria-hidden")).toBe("true");
      for (let i = 0; i < 2; i++) {
        act(() => void vi.advanceTimersByTime(50));
        withHelpers([h("a", "x", 1)]);
        view.rerender(<PlaygroundView onOpen={() => {}} />);
      }
      act(() => void vi.advanceTimersByTime(350));
      expect(view.container.querySelectorAll(".pg-helper--leaving")).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows a short label under each helper, the role for the generic look", () => {
    withHelpers([h("a", "code-researcher", 1), h("b", "implementer", 2), h("c", "a-very-long-role-name", 3), h("d", "", 4)]);
    const { container } = render(<PlaygroundView onOpen={() => {}} />);
    const labels = live(container).map((e) => e.querySelector(".pg-helper-label"));
    expect(labels.map((l) => l?.textContent)).toEqual([t("Investigador"), t("Implementador"), "a-very-long-role-name", t("Ayudante")]);
    expect(labels[2]!.getAttribute("title")).toBe("a-very-long-role-name");
  });

  it("keeps a leaving helper's spot reserved until it is removed", () => {
    vi.useFakeTimers();
    try {
      withHelpers([h("a", "x", 1), h("b", "y", 2)]);
      const view = render(<PlaygroundView onOpen={() => {}} />);
      withHelpers([h("a", "x", 1), h("n", "new", 3)]);
      view.rerender(<PlaygroundView onOpen={() => {}} />);
      const spot = (id: string) => live(view.container).find((e) => e.getAttribute("aria-label")?.startsWith(id))?.getAttribute("data-spot");
      const leaving = view.container.querySelector(".pg-helper--leaving")!.getAttribute("data-spot");
      expect(leaving).toBe("1");
      expect(spot("new")).not.toBe(leaving);
      act(() => void vi.advanceTimersByTime(350));
      expect(view.container.querySelectorAll(".pg-helper--leaving")).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("stale and hidden office", () => {
  const setVisibility = (state: "visible" | "hidden") => {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => state });
    Object.defineProperty(document, "hidden", { configurable: true, get: () => state === "hidden" });
    document.dispatchEvent(new Event("visibilitychange"));
  };
  afterEach(() => setVisibility("visible"));

  it("dims the last picture under a reconnecting banner while offline", () => {
    vi.mocked(usePlayground).mockReturnValue({ areas: [{ project: "web", desks: [desk("W-1", 0, "working")] }], online: false });
    const { container, getByText, rerender } = render(<PlaygroundView onOpen={() => {}} />);
    expect(container.querySelector(".pg-office")!.classList.contains("pg-office--stale")).toBe(true);
    expect(getByText(t("Reconectando…"))).toBeTruthy();
    expect(container.textContent).toContain("title of W-1");
    show([{ project: "web", desks: [desk("W-1", 0, "working")] }]);
    rerender(<PlaygroundView onOpen={() => {}} />);
    expect(container.querySelector(".pg-office--stale")).toBeNull();
    expect(container.textContent).not.toContain(t("Reconectando…"));
  });

  it("pauses the animations while the document is hidden", () => {
    show([{ project: "web", desks: [desk("W-1", 0, "working")] }]);
    const { container } = render(<PlaygroundView onOpen={() => {}} />);
    const office = () => container.querySelector(".pg-office")!;
    expect(office().classList.contains("pg-office--paused")).toBe(false);
    act(() => setVisibility("hidden"));
    expect(office().classList.contains("pg-office--paused")).toBe(true);
    act(() => setVisibility("visible"));
    expect(office().classList.contains("pg-office--paused")).toBe(false);
  });
});
