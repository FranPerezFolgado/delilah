// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { setLanguage } from "../../i18n";
beforeEach(() => setLanguage("es"));
afterEach(() => { cleanup(); localStorage.clear(); });

import { StatusBadge } from "./StatusBadge";
import type { TaskStatus } from "../../api";

const cases: [TaskStatus | "backlog", string, string][] = [
  ["backlog", "neutral", "Backlog"],
  [{ type: "queued" } as TaskStatus, "neutral", "Backlog"],
  [{ type: "preparing-workspace" } as TaskStatus, "run", "Trabajando"],
  [{ type: "running" } as TaskStatus, "run", "Trabajando"],
  [{ type: "waiting-for-user" } as TaskStatus, "wait", "Te espera"],
  [{ type: "paused-by-limit", resume_at: "2026-01-01T10:30:00" } as TaskStatus, "pause", "En pausa"],
  [{ type: "done" } as TaskStatus, "ok", "Hecha"],
  [{ type: "failed", reason: "boom" } as TaskStatus, "err", "Error"],
  [{ type: "setup-failed", reason: "no git" } as TaskStatus, "err", "Error"],
];

describe("StatusBadge", () => {
  it.each(cases)("%j -> %s / %s", (status, tone, text) => {
    const { container } = render(<StatusBadge status={status} />);
    const el = container.querySelector(".badge")!;
    expect(el.classList.contains(tone)).toBe(true);
    expect(el.textContent).toBe(text);
  });
  it("title carries the reason and the resume time", () => {
    const { container } = render(<StatusBadge status={{ type: "failed", reason: "boom" } as TaskStatus} />);
    expect(container.querySelector(".badge")!.getAttribute("title")).toBe("boom");
    cleanup();
    const p = render(<StatusBadge status={{ type: "paused-by-limit", resume_at: "2026-01-01T10:30:00" } as TaskStatus} />);
    expect(p.container.querySelector(".badge")!.getAttribute("title")).toBe("se reanuda a las 10:30");
  });
  it("detail shows it inline", () => {
    const { container } = render(<StatusBadge status={{ type: "failed", reason: "boom" } as TaskStatus} detail />);
    expect(container.textContent).toBe("Error · boom");
  });
});
