// Keyboard shortcuts: the actions, their default keys, and how a key press is matched.
// A combo is written like "Mod+Shift+T": Mod is ⌘ on macOS and Ctrl elsewhere.

export type ActionId =
  | "new-terminal"
  | "new-claude"
  | "close-session"
  | "next-pane"
  | "prev-pane"
  | "toggle-tree"
  | "quick-open"
  | "new-task"
  | "view-inbox"
  | "view-tasks"
  | "view-jira"
  | "view-projects";

export interface ActionDef {
  id: ActionId;
  label: string;
  group: string;
  default: string;
}

export const ACTIONS: ActionDef[] = [
  { id: "new-terminal", label: "Nueva terminal en el workspace", group: "Sesiones", default: "Mod+T" },
  { id: "new-claude", label: "Nueva sesión de Claude", group: "Sesiones", default: "Mod+Shift+T" },
  { id: "close-session", label: "Cerrar la pestaña activa", group: "Tarea", default: "Mod+W" },
  { id: "next-pane", label: "Pestaña siguiente", group: "Tarea", default: "Ctrl+Tab" },
  { id: "prev-pane", label: "Pestaña anterior", group: "Tarea", default: "Ctrl+Shift+Tab" },
  { id: "toggle-tree", label: "Mostrar u ocultar el árbol de archivos", group: "Tarea", default: "Mod+B" },
  { id: "quick-open", label: "Abrir un archivo por su nombre", group: "Tarea", default: "Mod+P" },
  { id: "new-task", label: "Nueva tarea", group: "General", default: "Mod+N" },
  { id: "view-inbox", label: "Ir a la Bandeja", group: "General", default: "Mod+I" },
  { id: "view-tasks", label: "Ir a Tareas", group: "General", default: "Mod+1" },
  { id: "view-jira", label: "Ir a Jira", group: "General", default: "Mod+2" },
  { id: "view-projects", label: "Ir a Proyectos", group: "General", default: "Mod+3" },
];

export type Bindings = Record<ActionId, string>;

export const DEFAULTS = Object.fromEntries(ACTIONS.map((a) => [a.id, a.default])) as Bindings;

export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.platform || navigator.userAgent);

let current: Bindings = { ...DEFAULTS };
let recording = false;

export function setBindings(b: Partial<Bindings>) {
  current = { ...DEFAULTS, ...b };
}

/** While the settings screen records a new combo, no action fires. */
export function setRecording(on: boolean) {
  recording = on;
}

/** The combo a key press makes, or null for a bare key (typing must reach the terminal). */
export function comboFromEvent(e: KeyboardEvent): string | null {
  if (["Meta", "Control", "Shift", "Alt"].includes(e.key)) return null;
  const mod = isMac ? e.metaKey : e.ctrlKey;
  const ctrl = isMac && e.ctrlKey;
  const fnKey = /^F\d{1,2}$/.test(e.key);
  if (!mod && !ctrl && !e.altKey && !fnKey) return null;
  // Letters and digits by position, so ⌘⇧1 is not read as ⌘!.
  let key = e.key;
  if (e.code.startsWith("Key")) key = e.code.slice(3);
  else if (e.code.startsWith("Digit")) key = e.code.slice(5);
  else if (key.length === 1) key = key.toUpperCase();
  const parts = [mod && "Mod", ctrl && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", key].filter(Boolean);
  return parts.join("+");
}

export function actionFor(e: KeyboardEvent): ActionId | null {
  if (recording) return null;
  const combo = comboFromEvent(e);
  if (!combo) return null;
  const hit = (Object.entries(current) as [ActionId, string][]).find(([, c]) => c === combo);
  return hit ? hit[0] : null;
}

const SYMBOLS: Record<string, string> = isMac
  ? { Mod: "⌘", Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Enter: "↵", Tab: "⇥", Backspace: "⌫", Escape: "⎋" }
  : { Mod: "Ctrl+", Ctrl: "Ctrl+", Alt: "Alt+", Shift: "Shift+", Enter: "Enter", Tab: "Tab" };

/** "Mod+Shift+T" → "⌘⇧T" on macOS. */
export function formatCombo(combo: string): string {
  return combo
    .split("+")
    .map((p) => SYMBOLS[p] ?? p)
    .join("");
}

export const keyOf = (id: ActionId) => formatCombo(current[id]);
