// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { setLanguage } from "../../i18n";
beforeEach(() => setLanguage("es"));
afterEach(() => { cleanup(); localStorage.clear(); });

import { PhaseBars } from "./PhaseBars";
import type { TaskStatus } from "../../api";

const running = { type: "running" } as TaskStatus;

describe("PhaseBars", () => {
  it("4 segments, the first `step` on, and the label", () => {
    const { container } = render(<PhaseBars phase={{ column: "plan", step: 2 }} status={running} />);
    expect(container.querySelectorAll(".bars i").length).toBe(4);
    expect(container.querySelectorAll(".bars i.on").length).toBe(2);
    expect(container.querySelector(".lbl")!.textContent).toBe("Plan · 2/4");
    const root = container.querySelector(".phasebars")!;
    expect(root.getAttribute("aria-label")).toBe("Plan · 2/4");
    expect(root.getAttribute("title")).toBe("Plan · 2/4");
  });
  it("Implementación shows the tasks count", () => {
    const { container } = render(
      <PhaseBars phase={{ column: "implement", step: 3, done: 6, total: 14 }} status={running} />,
    );
    expect(container.querySelector(".lbl")!.textContent).toBe("Implementación · 3/4 · 6/14 tareas");
  });
  it("no count without a total", () => {
    const { container } = render(<PhaseBars phase={{ column: "implement", step: 3 }} status={running} />);
    expect(container.querySelector(".lbl")!.textContent).toBe("Implementación · 3/4");
  });
  it("mini and labelInTooltip", () => {
    const { container } = render(<PhaseBars phase={{ column: "plan", step: 2 }} status={running} mini labelInTooltip />);
    expect(container.querySelector(".bars.mini")).not.toBeNull();
    expect(container.querySelector(".lbl")).toBeNull();
    expect(container.querySelector(".phasebars")!.getAttribute("title")).toBe("Plan · 2/4");
  });
  it("nothing for done", () => {
    const { container } = render(<PhaseBars phase={{ column: "review", step: 4 }} status={{ type: "done" } as TaskStatus} />);
    expect(container.firstChild).toBeNull();
  });
  it("Sin iniciar for setup-failed, no bar on", () => {
    const { container } = render(<PhaseBars phase={null} status={{ type: "setup-failed", reason: "x" } as TaskStatus} />);
    expect(container.querySelector(".lbl")!.textContent).toBe("Sin iniciar");
    expect(container.querySelectorAll(".bars i.on").length).toBe(0);
  });
  it("nothing without a phase", () => {
    expect(render(<PhaseBars phase={null} status={running} />).container.firstChild).toBeNull();
  });
  it("is not clickable", () => {
    const { container } = render(<PhaseBars phase={{ column: "plan", step: 2 }} status={running} />);
    expect(container.querySelector("button, a, [role=button]")).toBeNull();
  });
});
