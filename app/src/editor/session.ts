// A task's screen state, kept outside React so that open tabs, unsaved edits, undo
// history and cursors survive going to another view and back.
import type { EditorState, Text } from "@codemirror/state";

/** The tabs that are not files. File tabs are keyed by their workspace path. */
export const OVERVIEW = "\0overview";
export const CHANGES = "\0changes";
/** A terminal tab: this prefix and the daemon's session id. */
export const SESSION = "\0session:";
export const sessionTab = (id: string) => SESSION + id;
export const sessionOf = (tab: string) => (tab.startsWith(SESSION) ? tab.slice(SESSION.length) : null);

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
  /** OVERVIEW, CHANGES, a session tab or a file path. */
  active: string;
  states: Map<string, EditorState>;
  /** Folders open in the tree. */
  expanded: Set<string>;
  /** Files that were open last time, still to load (see `restore`). */
  pending: string[];
}

// What a task had open, so its tabs come back after reloading the app. Per viewer:
// browser storage, and losing it only means reopening some tabs.
interface Saved {
  active: string;
  files: string[];
  changesOpen: boolean;
  expanded: string[];
}
const savedKey = (task: string) => `delilah.tabs.${task}`;

function restore(task: string): Saved | null {
  try {
    const raw = localStorage.getItem(savedKey(task));
    return raw ? (JSON.parse(raw) as Saved) : null;
  } catch {
    return null;
  }
}

/** Remembers what a task has open. Call it whenever its tabs change. */
export function persist(task: string, s: TaskSession) {
  const saved: Saved = {
    active: s.active,
    files: [...s.tabs.map((t) => t.path), ...s.pending],
    changesOpen: s.changesOpen,
    expanded: [...s.expanded],
  };
  try {
    localStorage.setItem(savedKey(task), JSON.stringify(saved));
  } catch {
    // No storage: the tabs just won't come back after a reload.
  }
}

const sessions = new Map<string, TaskSession>();

export function sessionFor(task: string): TaskSession {
  let s = sessions.get(task);
  if (!s) {
    const saved = restore(task);
    s = {
      tabs: [],
      changesOpen: saved?.changesOpen ?? false,
      active: saved?.active ?? OVERVIEW,
      states: new Map(),
      expanded: new Set(saved?.expanded ?? []),
      pending: saved?.files ?? [],
    };
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
