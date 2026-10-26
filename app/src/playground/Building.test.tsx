// @vitest-environment jsdom
import { t } from "../i18n";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { FloorModel } from "./useOffice";
import { Building } from "./Building";
import { useOffice } from "./useOffice";

vi.mock("./useOffice");
vi.mock("./FloorMini", () => ({
  FloorMini: (p: { floor: FloorModel; onOpen: (o: string) => void }) => (
    <button data-testid="floor" onClick={() => p.onOpen(p.floor.owner)}>
      {p.floor.owner}
    </button>
  ),
}));

const floor = (owner: string, waiting = 0): FloorModel => ({ owner, title: owner, phase: null, project: "web", leads: [], helpers: [], leaving: [], waiting });
function office(floors: FloorModel[], online = true) {
  vi.mocked(useOffice).mockReturnValue({ floors, online, floorOf: (o) => floors.find((f) => f.owner === o)! });
}
const owners = (c: HTMLElement) => [...c.querySelectorAll("[data-testid=floor]")].map((e) => e.textContent);

afterEach(cleanup);

describe("Building", () => {
  it("lists a floor per owner, newest on top", () => {
    office([floor("A"), floor("B"), floor("C")]);
    const { container } = render(<Building onOpenFloor={() => {}} />);
    expect(owners(container)).toEqual(["C", "B", "A"]);
  });

  it("keeps the order when the state changes and puts a new floor on top", () => {
    office([floor("A"), floor("B")]);
    const view = render(<Building onOpenFloor={() => {}} />);
    office([floor("B", 1), floor("A")]);
    view.rerender(<Building onOpenFloor={() => {}} />);
    expect(owners(view.container)).toEqual(["B", "A"]);
    office([floor("B"), floor("A"), floor("N")]);
    view.rerender(<Building onOpenFloor={() => {}} />);
    expect(owners(view.container)).toEqual(["N", "B", "A"]);
  });

  it("totals the waiting agents", () => {
    office([floor("A", 2), floor("B", 1)]);
    const { getByText } = render(<Building onOpenFloor={() => {}} />);
    expect(getByText(t("{n} esperando", { n: 3 }))).toBeTruthy();
  });

  it("says so when no agent is working", () => {
    office([]);
    const { getByText, queryAllByTestId } = render(<Building onOpenFloor={() => {}} />);
    expect(getByText(t("Ningún agente trabajando"))).toBeTruthy();
    expect(queryAllByTestId("floor")).toHaveLength(0);
  });

  it("opens the floor that was clicked", () => {
    office([floor("A"), floor("B")]);
    const onOpenFloor = vi.fn();
    const { getAllByTestId } = render(<Building onOpenFloor={onOpenFloor} />);
    fireEvent.click(getAllByTestId("floor")[1]);
    expect(onOpenFloor).toHaveBeenCalledWith("A");
  });

  it("shows a banner and dims the building when the connection is lost", () => {
    office([floor("A")], false);
    const { getByRole, container } = render(<Building onOpenFloor={() => {}} />);
    expect(getByRole("status").textContent).toBe(t("Reconectando…"));
    expect(container.querySelector(".bld--stale")).toBeTruthy();
  });

  it("pauses while the page is hidden", () => {
    office([floor("A")]);
    const spy = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    const { container } = render(<Building onOpenFloor={() => {}} />);
    expect(container.querySelector(".bld--paused")).toBeTruthy();
    spy.mockRestore();
  });

  it("renders the extra header content", () => {
    office([]);
    const { getByText } = render(<Building onOpenFloor={() => {}} headerExtra={<button>Extra</button>} />);
    expect(getByText("Extra")).toBeTruthy();
  });
});
