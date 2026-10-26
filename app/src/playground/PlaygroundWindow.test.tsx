// @vitest-environment jsdom
import { t } from "../i18n";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { requestOpenTask } from "../platform";
import { connect } from "../live";
import { PlaygroundWindow } from "./PlaygroundWindow";

vi.mock("../live", () => ({ connect: vi.fn(() => () => {}) }));
vi.mock("../platform", () => ({ requestOpenTask: vi.fn(), openPlaygroundWindow: vi.fn() }));
vi.mock("./Building", () => ({
  Building: (p: { onOpenFloor: (o: string) => void; online?: boolean }) => (
    <button data-online={String(p.online)} onClick={() => p.onOpenFloor("T-7")}>building</button>
  ),
}));
vi.mock("./OfficeTab", () => ({
  OfficeTab: (p: { owner: string; online?: boolean; onOpenSession: (id: string) => void }) => (
    <button data-online={String(p.online)} onClick={() => p.onOpenSession("s9")}>floor {p.owner}</button>
  ),
}));

afterEach(() => {
  cleanup();
  vi.mocked(connect).mockClear();
});

describe("PlaygroundWindow", () => {
  it("holds the only connection and hands its state to the building and the floor", () => {
    let status: (c: boolean) => void = () => {};
    vi.mocked(connect).mockImplementationOnce((cb) => {
      status = cb;
      return () => {};
    });
    const { getByText } = render(<PlaygroundWindow />);
    expect(connect).toHaveBeenCalledTimes(1);
    expect(getByText("building").getAttribute("data-online")).toBe("true");
    act(() => status(false));
    expect(getByText("building").getAttribute("data-online")).toBe("false");
    fireEvent.click(getByText("building"));
    expect(getByText("floor T-7").getAttribute("data-online")).toBe("false");
    expect(connect).toHaveBeenCalledTimes(1);
  });

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
