import { describe, expect, it } from "vitest";
import { AGENT_NEW_LINE, isAgentNewLineKey, isSendKey } from "./keys";

const key = (k: string, mods: Partial<KeyboardEvent> & { isComposing?: boolean } = {}) => ({
  key: k,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ctrlKey: false,
  ...mods,
});

describe("isSendKey", () => {
  it("sends on Enter", () => {
    expect(isSendKey(key("Enter"))).toBe(true);
  });

  it("starts a new line on Shift+Enter and Option+Enter", () => {
    expect(isSendKey(key("Enter", { shiftKey: true }))).toBe(false);
    expect(isSendKey(key("Enter", { altKey: true }))).toBe(false);
  });

  it("does not send on an Enter that confirms an IME composition", () => {
    expect(isSendKey(key("Enter", { isComposing: true }))).toBe(false);
  });

  it("does not send on other keys", () => {
    expect(isSendKey(key("a"))).toBe(false);
  });
});

describe("isAgentNewLineKey", () => {
  it("makes Shift+Enter a new line in an agent's terminal", () => {
    expect(isAgentNewLineKey(key("Enter", { shiftKey: true }), true)).toBe(true);
    expect(AGENT_NEW_LINE).toBe("\x1b\r");
  });

  it("leaves Shift+Enter as Enter in a plain shell", () => {
    expect(isAgentNewLineKey(key("Enter", { shiftKey: true }), false)).toBe(false);
  });

  it("leaves Enter alone, or with other modifiers, to xterm and the app", () => {
    expect(isAgentNewLineKey(key("Enter"), true)).toBe(false);
    for (const mod of ["metaKey", "ctrlKey", "altKey"]) {
      expect(isAgentNewLineKey(key("Enter", { shiftKey: true, [mod]: true }), true)).toBe(false);
    }
  });
});
