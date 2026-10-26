// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { setLanguage } from "../../i18n";
beforeEach(() => setLanguage("es"));
afterEach(() => { cleanup(); localStorage.clear(); });

import { InboxKind } from "./InboxKind";

describe("InboxKind", () => {
  it.each([
    ["permission", "Permiso"],
    ["question", "Pregunta"],
    ["waiting", "Pregunta"],
    ["turn-done", "Aviso"],
    ["pr-opened", "Aviso"],
  ] as const)("%s -> %s with an icon", (k, text) => {
    const { container } = render(<InboxKind kind={k} />);
    const el = container.querySelector(".kind")!;
    expect(el.textContent).toBe(text);
    expect(el.querySelector("svg")).not.toBeNull();
  });
});
