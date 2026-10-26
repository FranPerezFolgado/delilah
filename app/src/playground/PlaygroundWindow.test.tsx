// @vitest-environment jsdom
import { t } from "../i18n";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { requestOpenTask } from "../platform";
import { PlaygroundWindow } from "./PlaygroundWindow";

vi.mock("../platform", () => ({ requestOpenTask: vi.fn(), openPlaygroundWindow: vi.fn() }));
vi.mock("./Building", () => ({
  Building: (p: { onOpenFloor: (o: string) => void }) => <button onClick={() => p.onOpenFloor("T-7")}>building</button>,
}));
vi.mock("./OfficeTab", () => ({
  OfficeTab: (p: { owner: string; onOpenSession: (id: string) => void }) => <button onClick={() => p.onOpenSession("s9")}>floor {p.owner}</button>,
}));

afterEach(cleanup);

describe("PlaygroundWindow", () => {
  it("starts on the building", () => {
    const { getByText, queryByText } = render(<PlaygroundWindow />);
    expect(getByText("building")).toBeTruthy();
    expect(queryByText(t("Volver al edificio"))).toBeNull();
  });

  it("opens a floor in place and goes back", () => {
    const { getByText, queryByText } = render(<PlaygroundWindow />);
    fireEvent.click(getByText("building"));
    expect(getByText("floor T-7")).toBeTruthy();
    expect(queryByText("building")).toBeNull();
    fireEvent.click(getByText(t("Volver al edificio")));
    expect(getByText("building")).toBeTruthy();
  });

  it("brings the main window to the lead's terminal", () => {
    const { getByText } = render(<PlaygroundWindow />);
    fireEvent.click(getByText("building"));
    fireEvent.click(getByText("floor T-7"));
    expect(requestOpenTask).toHaveBeenCalledWith({ task: "T-7", session: "s9" });
  });
});
