import { test } from "node:test";
import assert from "node:assert/strict";
import { AGENT_NEW_LINE, isAgentNewLineKey, isSendKey } from "../src/keys.ts";

const key = (k, mods = {}) => ({ key: k, shiftKey: false, altKey: false, metaKey: false, ctrlKey: false, ...mods });

test("Enter sends a reply", () => {
  assert.equal(isSendKey(key("Enter")), true);
});

test("Shift+Enter and Option+Enter start a new line in a reply", () => {
  assert.equal(isSendKey(key("Enter", { shiftKey: true })), false);
  assert.equal(isSendKey(key("Enter", { altKey: true })), false);
});

test("an Enter that confirms an IME composition does not send", () => {
  assert.equal(isSendKey(key("Enter", { isComposing: true })), false);
});

test("other keys do not send", () => {
  assert.equal(isSendKey(key("a")), false);
});

test("Shift+Enter is a new line in an agent's terminal", () => {
  assert.equal(isAgentNewLineKey(key("Enter", { shiftKey: true }), true), true);
  assert.equal(AGENT_NEW_LINE, "\x1b\r");
});

test("a plain shell keeps Shift+Enter as Enter", () => {
  assert.equal(isAgentNewLineKey(key("Enter", { shiftKey: true }), false), false);
});

test("Enter alone, or with other modifiers, is left to xterm and the app", () => {
  assert.equal(isAgentNewLineKey(key("Enter"), true), false);
  for (const mod of ["metaKey", "ctrlKey", "altKey"]) {
    assert.equal(isAgentNewLineKey(key("Enter", { shiftKey: true, [mod]: true }), true), false);
  }
});
