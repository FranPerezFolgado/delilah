// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { Session } from "../api";
import type { DeskModel, ProjectArea } from "./layout";
import { t } from "../i18n";
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
  },
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

afterEach(cleanup);

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
});
