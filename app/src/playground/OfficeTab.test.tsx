// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { OfficeTab } from "./OfficeTab";

const floor = vi.hoisted(() => ({ owner: "T-1" }));
vi.mock("./useOffice", () => ({ useFloor: (owner: string) => ({ floor: { ...floor, owner }, online: true }) }));
vi.mock("./Floor", () => ({
  Floor: (p: { floor: { owner: string }; onOpenSession: (id: string) => void }) => (
    <button data-testid="floor" onClick={() => p.onOpenSession("s1")}>{p.floor.owner}</button>
  ),
}));

afterEach(cleanup);

it("renders the owner's floor and opens a lead's session", () => {
  const onOpenSession = vi.fn();
  render(<OfficeTab owner="T-9" onOpenSession={onOpenSession} />);
  fireEvent.click(screen.getByTestId("floor"));
  expect(screen.getByTestId("floor").textContent).toBe("T-9");
  expect(onOpenSession).toHaveBeenCalledWith("s1");
});
