// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, ApiError, type InboxItem } from "../api";
import * as platform from "../platform";
import { InboxView } from "./InboxView";

const item: InboxItem = {
  kind: "pr-failed",
  task: "PROJ-1",
  title: "Reintentos",
  session: null,
  text: "CI falló: test",
  since: null,
  pr: { notice: 7, url: "https://github.com/o/r/pull/3", repo: "r" },
};

const view = (onChanged = () => {}) =>
  render(<InboxView items={[item]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("InboxView PR items", () => {
  it("forwards the notice to the agent", async () => {
    const fwd = vi.spyOn(api, "forwardNotice").mockResolvedValue(null);
    const onChanged = vi.fn();
    view(onChanged);
    fireEvent.click(screen.getByText("Ask the agent to fix it"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(fwd).toHaveBeenCalledWith(7);
  });

  it("shows the daemon's message and disables the button on 409", async () => {
    vi.spyOn(api, "forwardNotice").mockRejectedValue(new ApiError("paused until 14:30", 409));
    view();
    fireEvent.click(screen.getByText("Ask the agent to fix it"));
    await waitFor(() => expect(screen.getByText("paused until 14:30")).toBeTruthy());
    expect((screen.getByText("Ask the agent to fix it") as HTMLButtonElement).disabled).toBe(true);
  });

  it("dismisses the notice", async () => {
    const dis = vi.spyOn(api, "dismissNotice").mockResolvedValue(null);
    view();
    fireEvent.click(screen.getByText("Discard"));
    await waitFor(() => expect(dis).toHaveBeenCalledWith(7));
  });

  it("opens the PR externally", () => {
    const open = vi.spyOn(platform, "openExternal").mockResolvedValue();
    view();
    fireEvent.click(screen.getByText("View PR"));
    expect(open).toHaveBeenCalledWith("https://github.com/o/r/pull/3");
  });
});
