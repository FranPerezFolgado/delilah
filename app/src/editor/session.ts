// The editor's open files per task, kept outside React so that unsaved edits, undo
// history and cursors survive going to another view and back.
import type { EditorState, Text } from "@codemirror/state";

export interface Tab {
  path: string;
  /** The version on disk the edit started from. */
  version: string;
  size: number;
  /** The text as last loaded or saved; null when the file cannot be edited here. */
  saved: Text | null;
  dirty: boolean;
  /** The file changed on disk while it had unsaved edits here. */
  conflict: boolean;
}

export interface EditorSession {
  tabs: Tab[];
  active: string | null;
  states: Map<string, EditorState>;
  /** Folders open in the tree. */
  expanded: Set<string>;
}

const sessions = new Map<string, EditorSession>();

export function sessionFor(task: string): EditorSession {
  let s = sessions.get(task);
  if (!s) {
    s = { tabs: [], active: null, states: new Map(), expanded: new Set() };
    sessions.set(task, s);
  }
  return s;
}

/** Whether any task has unsaved edits (e.g. to warn before quitting). */
export const hasUnsaved = () => [...sessions.values()].some((s) => s.tabs.some((t) => t.dirty));
