// @vitest-environment jsdom
import { t } from "../i18n";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { openPlaygroundWindow } from "../platform";
import { PlaygroundView } from "./PlaygroundView";

vi.mock("./Building", () => ({
  Building: (p: { onOpenFloor: (o: string) => void; headerExtra?: unknown; online?: boolean }) => (
    <div data-testid="building" data-online={String(p.online)}>
      <button onClick={() => p.onOpenFloor("T-1")}>floor</button>
      {p.headerExtra as never}
    </div>
  ),
}));
vi.mock("../platform", () => ({ openPlaygroundWindow: vi.fn() }));

afterEach(cleanup);

describe("PlaygroundView", () => {
  it("renders the building and opens a floor", () => {
    const onOpenFloor = vi.fn();
    const { getByText } = render(<PlaygroundView onOpenFloor={onOpenFloor} online />);
    fireEvent.click(getByText("floor"));
    expect(onOpenFloor).toHaveBeenCalledWith("T-1");
  });

  it("offers the window button, except in the window itself", () => {
    const { getByText, queryByText, rerender } = render(<PlaygroundView onOpenFloor={() => {}} />);
    fireEvent.click(getByText(t("Abrir en ventana")));
    expect(openPlaygroundWindow).toHaveBeenCalledTimes(1);
    rerender(<PlaygroundView onOpenFloor={() => {}} standalone />);
    expect(queryByText(t("Abrir en ventana"))).toBeNull();
  });
});
