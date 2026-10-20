// What Enter does in the places where text goes to an agent. Kept apart from the
// components so the rules can be tested without a browser.

type KeyLike = Pick<KeyboardEvent, "key" | "shiftKey" | "altKey" | "metaKey" | "ctrlKey"> & {
  isComposing?: boolean;
};

/** In the reply box, Enter sends; Shift+Enter or ⌥Enter start a new line, and an
 *  Enter that confirms an IME composition is not a send. */
export function isSendKey(e: KeyLike): boolean {
  return e.key === "Enter" && !e.shiftKey && !e.altKey && !e.isComposing;
}

/** ESC CR: what Claude Code takes as a new line (as ⌥Enter sends). xterm would
 *  send a bare CR for Shift+Enter, which submits. */
export const AGENT_NEW_LINE = "\x1b\r";

/** Shift+Enter in an agent's terminal is a new line, not a send. A plain shell
 *  keeps the usual meaning. */
export function isAgentNewLineKey(e: KeyLike, agent: boolean): boolean {
  return agent && e.key === "Enter" && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey;
}
