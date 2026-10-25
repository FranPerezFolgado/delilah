// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { ACTIONS, actionFor, DEFAULTS, setBindings } from "./shortcuts";
import { EN } from "./i18n/en/shortcuts";

const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);
const mod = { metaKey: true, ctrlKey: true };

describe("toggle-sidebar", () => {
  afterEach(() => setBindings({}));

  it("is in the Ventana group, bound to Mod+\\", () => {
    const a = ACTIONS.find((x) => x.id === "toggle-sidebar");
    expect(a?.group).toBe("Ventana");
    expect(a?.label).toBe("Plegar barra lateral");
    expect(DEFAULTS["toggle-sidebar"]).toBe("Mod+\\");
  });

  it("fires on the default key", () => {
    expect(actionFor(key({ ...mod, key: "\\", code: "Backslash" }))).toBe("toggle-sidebar");
  });

  it("can be rebound", () => {
    setBindings({ "toggle-sidebar": "Mod+J" });
    expect(actionFor(key({ ...mod, key: "j", code: "KeyJ" }))).toBe("toggle-sidebar");
    expect(actionFor(key({ ...mod, key: "\\", code: "Backslash" }))).toBeNull();
  });

  it("has English for its label and group", () => {
    expect(EN["Ventana"]).toBeTruthy();
    expect(EN["Plegar barra lateral"]).toBeTruthy();
  });
});
