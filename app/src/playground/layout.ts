import type { Action, Activity, Helper, Session } from "../api";
import { t, tn } from "../i18n";
import { type Column } from "../phases";
import { formatTokens } from "./art/kinds";

/** What a desk's character is doing, from the session's state. */
export type DeskState = "starting" | "thinking" | "waiting" | "paused" | "edit" | "read" | "run" | "delegate" | "other" | "supervising";

type Look = "researcher" | "implementer" | "reviewer" | "generic";

export interface DeskModel {
  session: Session;
  /** The session's owner: a task key or `@<project>`. */
  owner: string;
  /** The task key, or the project's name for a project session. */
  key: string;
  title: string;
  phase: Column | null;
  project: string | null;
  /** The desk's place in its project's grid; stable while the session lives. */
  slot: number;
  /** What the session is doing now; none when idle or unknown. */
  activity?: Activity;
}

/** What a desk's character is doing: the session's state, and while it works, its action. */
export function deskState(session: Session, activity?: Activity): DeskState {
  switch (session.state) {
    case "starting":
    case "waiting":
    case "paused":
      return session.state;
    default: {
      const action = activity?.action;
      if (!action) return "thinking";
      if (action.kind === "delegate") return action.background ? "thinking" : "supervising";
      return action.kind;
    }
  }
}

/** A lead desk's state on the office floor: the session's state, working while it runs. */
export type FloorState = "starting" | "working" | "waiting" | "paused";

/** The office table's state for a session (the action is shown separately). */
export function floorState(session: Session): FloorState {
  switch (session.state) {
    case "starting":
    case "waiting":
    case "paused":
      return session.state;
    default:
      return "working";
  }
}

/** The action as a short line: `Edit · api.rs`, or just the tool without a target. */
export function caption(action: Action | null | undefined): string | null {
  if (!action) return null;
  // Concatenated, not a template literal: the i18n coverage scan pairs backticks naively.
  return action.target ? action.tool + " · " + action.target : action.tool;
}

const STATE_WORD: Record<DeskState, string> = {
  starting: "Arrancando",
  thinking: "Pensando",
  waiting: "Esperándote",
  paused: "En pausa",
  edit: "Editando",
  read: "Leyendo",
  run: "Ejecutando",
  delegate: "Supervisando",
  supervising: "Supervisando",
  other: "Trabajando",
};

const LOOK_OF: Record<string, Look> = {
  "code-researcher": "researcher",
  Explore: "researcher",
  "doc-searcher": "researcher",
  implementer: "implementer",
  "code-quality-reviewer": "reviewer",
  "spec-compliance-reviewer": "reviewer",
};

/** How a helper looks and is called, from its role; an empty role is just "Ayudante". */
export function roleLook(role: string): { look: Look; label: string } {
  return { look: LOOK_OF[role] ?? "generic", label: role || t("Ayudante") };
}

const HELPER_WORD: Record<Action["kind"], string> = {
  edit: "Editando",
  read: "Leyendo",
  run: "Ejecutando",
  delegate: "Supervisando",
  other: "Trabajando",
};

/** "code-researcher leyendo": a helper's role and what it is doing. */
export const helperName = (h: Helper): string =>
  `${roleLook(h.role).label} ${t(h.action ? HELPER_WORD[h.action.kind] : "Pensando").toLowerCase()}`;

/** The character's accessible name: the task (or project), what it is doing, the action, the helpers and the tokens used. */
export function accessibleName(desk: DeskModel, activity?: Activity, tokens?: number): string {
  const state = deskState(desk.session, activity);
  const cap = state === "waiting" ? null : caption(activity?.action);
  const helpers = activity?.helpers ?? [];
  const extra = helpers.length
    ? `, ${tn(helpers.length, "{n} ayudante", "{n} ayudantes")}: ${[...helpers].sort((a, b) => a.since.localeCompare(b.since)).map(helperName).join(", ")}`
    : "";
  const used = tokens ? `, ${formatTokens(tokens)} tokens` : "";
  return `${desk.key}, ${t(STATE_WORD[state])}${cap ? `, ${cap}` : ""}${extra}${used}`;
}
