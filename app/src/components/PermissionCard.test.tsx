// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api } from "../api";
import { setLanguage } from "../i18n";
import { PermissionCard } from "./PermissionCard";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// The cases of the card itself live in AskCard.test.tsx; this one is a thin wrapper.
it("is AskCard's permission card for a session", async () => {
  const spy = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
  const done = vi.fn();
  render(<PermissionCard sessionId="s1" ask={{ id: "p1", tool: "Bash", summary: "npm test", suggestion: null, rule: null }} onDone={done} />);
  expect(screen.getByText("npm test")).toBeTruthy();
  fireEvent.click(screen.getByText("Permitir una vez"));
  await waitFor(() => expect(done).toHaveBeenCalled());
  expect(spy).toHaveBeenCalledWith("s1", "p1", "allow_once", undefined);
});
