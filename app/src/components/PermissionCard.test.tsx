// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, ApiError } from "../api";
import { setLanguage } from "../i18n";
import { PermissionCard } from "./PermissionCard";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const ask = { id: "p1", tool: "Bash", summary: "npm test", suggestion: null, rule: "Bash(npm test)" };

describe("PermissionCard", () => {
  it("shows the tool and the summary as plain text", () => {
    render(<PermissionCard sessionId="s1" ask={{ ...ask, summary: "<b>x</b>" }} />);
    expect(screen.getByText("Bash")).toBeTruthy();
    expect(screen.getByText("<b>x</b>")).toBeTruthy();
  });

  it("allows once", async () => {
    const spy = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    const done = vi.fn();
    render(<PermissionCard sessionId="s1" ask={ask} onDone={done} />);
    fireEvent.click(screen.getByText("Permitir una vez"));
    await waitFor(() => expect(done).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith("s1", "p1", "allow_once", undefined);
  });

  it("offers allow always with the rule shown, only when there is a rule", async () => {
    const spy = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    const { unmount } = render(<PermissionCard sessionId="s1" ask={ask} />);
    expect(screen.getByText("Bash(npm test)")).toBeTruthy();
    fireEvent.click(screen.getByText("Permitir siempre en esta tarea"));
    await waitFor(() => expect(spy).toHaveBeenCalledWith("s1", "p1", "allow_always", undefined));
    unmount();
    render(<PermissionCard sessionId="s1" ask={{ ...ask, rule: null }} />);
    expect(screen.queryByText("Permitir siempre en esta tarea")).toBeNull();
    expect(screen.getByText("Permitir una vez")).toBeTruthy();
    expect(screen.getByText("Denegar")).toBeTruthy();
  });

  it("denies with an optional reason", async () => {
    const spy = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    render(<PermissionCard sessionId="s1" ask={ask} />);
    fireEvent.change(screen.getByLabelText("Motivo (opcional)"), { target: { value: "no toques eso" } });
    fireEvent.click(screen.getByText("Denegar"));
    await waitFor(() => expect(spy).toHaveBeenCalledWith("s1", "p1", "deny", "no toques eso"));
  });

  it("shows the error inline and re-enables the buttons", async () => {
    vi.spyOn(api, "answerPermission").mockRejectedValue(new ApiError("no rule to remember for this request", 400));
    const done = vi.fn();
    render(<PermissionCard sessionId="s1" ask={ask} onDone={done} />);
    fireEvent.click(screen.getByText("Permitir siempre en esta tarea"));
    expect(await screen.findByText("no rule to remember for this request")).toBeTruthy();
    expect(done).not.toHaveBeenCalled();
    expect((screen.getByText("Denegar") as HTMLButtonElement).disabled).toBe(false);
  });
});
