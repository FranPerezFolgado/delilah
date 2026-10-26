// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { t } from "../i18n";
import { OfficeTab } from "./OfficeTab";

const floor = vi.hoisted(() => ({ owner: "T-1" }));
vi.mock("./useOffice", () => ({ useFloor: (owner: string) => ({ floor: { ...floor, owner }, online: true }) }));
vi.mock("./Floor", () => ({
  Floor: (p: { floor: { owner: string }; onOpenSession: (id: string) => void; headerExtra?: React.ReactNode }) => (
    <div>
      {p.headerExtra}
      <button data-testid="floor" onClick={() => p.onOpenSession("s1")}>{p.floor.owner}</button>
    </div>
  ),
}));
vi.mock("./FlowView", () => ({
  FlowView: (p: { owner: string; headerExtra?: React.ReactNode }) => (
    <div>
      {p.headerExtra}
      <span data-testid="flow">{p.owner}</span>
    </div>
  ),
}));

beforeEach(() => localStorage.clear());
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("renders the owner's floor and opens a lead's session", () => {
  const onOpenSession = vi.fn();
  render(<OfficeTab owner="T-9" onOpenSession={onOpenSession} />);
  fireEvent.click(screen.getByTestId("floor"));
  expect(screen.getByTestId("floor").textContent).toBe("T-9");
  expect(onOpenSession).toHaveBeenCalledWith("s1");
});

it("the toggle switches to Flujo and back", () => {
  render(<OfficeTab owner="T-9" onOpenSession={() => {}} />);
  expect(screen.getByRole("tablist")).toBeTruthy();
  expect(screen.getByRole("tab", { name: t("Oficina") }).getAttribute("aria-selected")).toBe("true");
  fireEvent.click(screen.getByRole("tab", { name: t("Flujo") }));
  expect(screen.getByTestId("flow").textContent).toBe("T-9");
  expect(screen.queryByTestId("floor")).toBeNull();
  fireEvent.click(screen.getByRole("tab", { name: t("Oficina") }));
  expect(screen.getByTestId("floor")).toBeTruthy();
});

it("persists the choice and shares it between tabs", () => {
  render(
    <>
      <OfficeTab owner="A" onOpenSession={() => {}} />
      <OfficeTab owner="B" onOpenSession={() => {}} />
    </>,
  );
  fireEvent.click(screen.getAllByRole("tab", { name: t("Flujo") })[0]);
  expect(localStorage.getItem("delilah.office.view")).toBe("flow");
  expect(screen.getAllByTestId("flow")).toHaveLength(2);
  cleanup();
  render(<OfficeTab owner="C" onOpenSession={() => {}} />);
  expect(screen.getByTestId("flow")).toBeTruthy();
});

it("falls back to Oficina when storage fails", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("denied");
  });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("denied");
  });
  render(<OfficeTab owner="T-9" onOpenSession={() => {}} />);
  expect(screen.getByTestId("floor")).toBeTruthy();
  fireEvent.click(screen.getByRole("tab", { name: t("Flujo") }));
  expect(screen.getByTestId("flow")).toBeTruthy();
});
