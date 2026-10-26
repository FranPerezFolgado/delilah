// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { setLanguage } from "./i18n";
import { historyEntry, historyLine } from "./history";

beforeEach(() => setLanguage("es"));

const SLUGS = [
  "start", "workspace-ready", "phase-reached", "agent-asked", "user-answered", "limit-reached", "reset-due",
  "manual-resume", "agent-finished", "mark-done", "fail", "start-aborted", "setup-failed", "retry-setup",
  "skip-setup", "created-from-conversation", "step-started", "autopilot-paused", "autopilot-resumed",
  "autopilot-answered", "autopilot-nudged", "autopilot-finished", "reopened", "runtime-switched",
  "permission-asked", "permission-expired", "agent-resumed",
  "autofix-sent", "autofix-fixed", "autofix-failed-again", "autofix-stopped", "autofix-cancelled",
];

describe("historyLine", () => {
  it("gives every core event a sentence that is not its slug", () => {
    for (const type of SLUGS) {
      const line = historyLine({ type });
      expect(line.length, type).toBeGreaterThan(3);
      expect(line, type).not.toBe(type);
      expect(line, type).not.toContain(type);
    }
  });

  it("labels the autofix events with the checks, the reason and the repo when it isn't the root", () => {
    setLanguage("en");
    const sent = { type: "autofix-sent", sha: "abc", attempt: 2, checks: ["test", "lint"] };
    expect(historyLine({ ...sent, repo: "." })).toBe("CI autofix: attempt 2 of 3 — test, lint");
    expect(historyLine({ ...sent, repo: "api" })).toBe("CI autofix: attempt 2 of 3 — test, lint · api");
    expect(historyLine({ ...sent, repo: ".", checks: [] })).toBe("CI autofix: attempt 2 of 3");
    expect(historyLine({ type: "autofix-fixed", repo: "." })).toBe("CI autofix: CI fixed");
    expect(historyLine({ type: "autofix-fixed", repo: "api" })).toBe("CI autofix: CI fixed · api");
    expect(historyLine({ type: "autofix-failed-again", repo: "." })).toBe("CI autofix: CI still failing");
    expect(historyLine({ type: "autofix-failed-again", repo: "api" })).toBe("CI autofix: CI still failing · api");
    expect(historyLine({ type: "autofix-stopped", repo: ".", reason: "limit" })).toBe("CI autofix stopped: reached the attempt limit");
    expect(historyLine({ type: "autofix-stopped", repo: "api", reason: "no-push" })).toBe("CI autofix stopped: the agent pushed no change · api");
    expect(historyLine({ type: "autofix-stopped", repo: ".", reason: "no-session" })).toBe("CI autofix stopped: no agent session");
    expect(historyLine({ type: "autofix-cancelled", repo: "." })).toBe("CI autofix cancelled");
    expect(historyLine({ type: "autofix-cancelled", repo: "api" })).toBe("CI autofix cancelled · api");
  });

  it("never shows the slug of an unknown event", () => {
    const line = historyLine({ type: "something-new" });
    expect(line).not.toContain("something-new");
    expect(line.length).toBeGreaterThan(3);
  });

  it("uses the event's fields", () => {
    expect(historyLine({ type: "reopened" })).toBe("Reabriste la tarea");
    expect(historyLine({ type: "runtime-switched", mode: "chat" })).toBe("La sesión pasó a chat");
    expect(historyLine({ type: "runtime-switched", mode: "terminal" })).toBe("La sesión pasó a la terminal");
    expect(historyLine({ type: "permission-asked", tool: "Bash", summary: "npm test" })).toBe("Claude pidió permiso para Bash: npm test");
    expect(historyLine({ type: "permission-expired", tool: "Bash", summary: "npm test", outcome: "terminal" })).toBe(
      "Caducó el permiso de Bash · se respondió en la terminal",
    );
    expect(historyLine({ type: "agent-resumed", reason: "background-done" })).toBe("Claude continúa: tarea en segundo plano terminada");
    expect(historyLine({ type: "agent-resumed", reason: null })).toBe("Claude continúa");
    expect(historyLine({ type: "fail", reason: "boom" })).toBe("Falló: boom");
    expect(historyLine({ type: "created-from-conversation", label: "Idea" })).toBe("Creada desde la conversación «Idea»");
  });

  it("translates", () => {
    setLanguage("en");
    expect(historyLine({ type: "reopened" })).toBe("You reopened the task");
    expect(historyLine({ type: "permission-expired", tool: "Bash", summary: "x", outcome: "terminal" })).toBe(
      "Permission for Bash expired · answered in the terminal",
    );
  });
});

describe("historyEntry", () => {
  it("pairs the sentence with a relative time", () => {
    const now = new Date("2026-03-10T12:00:00Z");
    const e = historyEntry({ at: "2026-03-10T11:56:00Z", event: { type: "start" } }, now);
    expect(e.text).toBe("Tarea creada");
    expect(e.when).toBe("hace 4 min");
  });
});
