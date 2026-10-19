// A task's screen state, kept outside React so that open tabs, unsaved edits, undo
// history and cursors survive going to another view and back.
import type { EditorState, Text } from "@codemirror/state";

/** The tabs that are not files. File tabs are keyed by their workspace path. */
export const OVERVIEW = "\0overview";
export const CHANGES = "\0changes";

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

export interface TaskSession {
  /** Open files, in tab order. */
  tabs: Tab[];
  changesOpen: boolean;
  /** OVERVIEW, CHANGES or a file path. */
  active: string;
  states: Map<string, EditorState>;
  /** Folders open in the tree. */
  expanded: Set<string>;
}

const sessions = new Map<string, TaskSession>();

export function sessionFor(task: string): TaskSession {
  let s = sessions.get(task);
  if (!s) {
    s = { tabs: [], changesOpen: false, active: OVERVIEW, states: new Map(), expanded: new Set() };
    sessions.set(task, s);
  }
  return s;
}

/** Whether any task has unsaved edits (e.g. to warn before quitting). */
export const hasUnsaved = () => [...sessions.values()].some((s) => s.tabs.some((t) => t.dirty));

// Whether the file tree is shown: a per-viewer convenience, so browser storage.
const TREE_KEY = "delilah.tree-open";
export function treeOpen(): boolean {
  try {
    return localStorage.getItem(TREE_KEY) !== "0";
  } catch {
    return true;
  }
}
export function setTreeOpen(open: boolean) {
  try {
    localStorage.setItem(TREE_KEY, open ? "1" : "0");
  } catch {
    // No storage: it just won't be remembered.
  }
}
