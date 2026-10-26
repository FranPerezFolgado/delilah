// Pure model of the activity history: agents, helpers and tool calls as graph nodes, plus a timeline.
import type { ActionKind, HistoryEntry, HistoryResponse } from "../api";

export type FlowState = "running" | "done" | "failed" | "unknown";

export interface FlowNode {
  id: string;
  type: "agent" | "helper" | "tool";
  label: string;
  kind?: ActionKind;
  tool?: string;
  target?: string | null;
  background?: boolean;
  state: FlowState;
  /** RFC 3339. */
  start: string;
  end?: string;
  /** The agent or helper that owns it (helpers: the session's lead). */
  parent?: string;
  session: string;
  /** A live agent that has reported no tool calls (an older agent, or nothing yet). */
  noCalls?: boolean;
}

export interface FlowEdge {
  from: string;
  to: string;
}

export interface TimelineRow {
  seq: number;
  at: string;
  node: string;
  session: string;
  event: "tool_started" | "tool_ended" | "helper_started" | "helper_ended" | "turn_over";
  kind?: ActionKind;
  tool?: string;
  target?: string | null;
  failed?: boolean;
  /** helper_started / helper_ended: the helper's role. */
  role?: string;
}

export interface FlowModel {
  nodes: FlowNode[];
  edges: FlowEdge[];
  timeline: TimelineRow[];
  notes: { dropped: boolean; empty: boolean; startedAt: string };
}

export const agentId = (session: string) => `s:${session}`;
export const helperId = (session: string, helper: string) => `h:${session}:${helper}`;
export const toolId = (session: string, toolUseId: string) => `t:${session}:${toolUseId}`;

const time = (at: string) => {
  const t = Date.parse(at);
  return Number.isNaN(t) ? 0 : t;
};

/** `liveSessions`: the owner's live Claude session ids; those without history still get a root node. */
export function buildFlow(history: HistoryResponse, liveSessions: string[] = []): FlowModel {
  const nodes: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  const timeline: TimelineRow[] = [];
  const byId = new Map<string, FlowNode>();
  const edgeKeys = new Set<string>();

  const add = (n: FlowNode) => {
    nodes.push(n);
    byId.set(n.id, n);
    return n;
  };
  const link = (from: string, to: string) => {
    const key = `${from}>${to}`;
    if (edgeKeys.has(key)) return;
    edgeKeys.add(key);
    edges.push({ from, to });
  };

  for (const s of history.sessions) {
    if (s.entries.length === 0) continue;
    const lead = agentId(s.session);
    add({
      id: lead,
      type: "agent",
      label: s.session,
      state: s.live ? "running" : "done",
      start: s.entries[0].at,
      session: s.session,
    });

    const ensureHelper = (id: string, role: string, at: string): FlowNode => {
      const hid = helperId(s.session, id);
      const existing = byId.get(hid);
      if (existing) {
        if (role) existing.label = role;
        return existing;
      }
      link(lead, hid);
      return add({
        id: hid,
        type: "helper",
        label: role || "helper",
        state: "running",
        start: at,
        parent: lead,
        session: s.session,
      });
    };
    const row = (e: HistoryEntry, node: string, extra: Partial<TimelineRow> = {}) =>
      timeline.push({ seq: e.seq, at: e.at, node, session: s.session, event: e.type, ...extra });

    for (const e of s.entries) {
      switch (e.type) {
        case "helper_started": {
          const h = ensureHelper(e.agent.id, e.agent.role, e.at);
          row(e, h.id, { role: h.label });
          break;
        }
        case "helper_ended": {
          const h = byId.get(helperId(s.session, e.id));
          if (!h) break;
          h.state = "done";
          h.end = e.at;
          row(e, h.id, { role: h.label });
          break;
        }
        case "tool_started": {
          const owner = e.agent ? ensureHelper(e.agent.id, e.agent.role, e.at).id : lead;
          const id = toolId(s.session, e.action.id);
          let n = byId.get(id);
          if (!n) {
            n = add({
              id,
              type: "tool",
              label: e.action.tool,
              kind: e.action.kind,
              tool: e.action.tool,
              target: e.action.target,
              background: e.action.background,
              state: "running",
              start: e.action.since || e.at,
              parent: owner,
              session: s.session,
            });
            link(owner, id);
          }
          row(e, id, { kind: n.kind, tool: n.tool, target: n.target });
          break;
        }
        case "tool_ended": {
          const n = byId.get(toolId(s.session, e.id));
          if (!n) break;
          n.state = e.failed ? "failed" : "done";
          n.end = e.at;
          row(e, n.id, { kind: n.kind, tool: n.tool, target: n.target, failed: e.failed });
          break;
        }
        case "turn_over":
          row(e, lead);
          break;
      }
    }

    if (!s.live) {
      // Nothing more will arrive: whatever never reported an end is unknown.
      for (const n of nodes) if (n.session === s.session && n.state === "running") n.state = "unknown";
    }
  }

  const seen = new Set(history.sessions.filter((s) => s.entries.length > 0).map((s) => s.session));
  for (const id of liveSessions) {
    if (seen.has(id)) continue;
    seen.add(id);
    add({ id: agentId(id), type: "agent", label: id, state: "running", start: history.started_at, session: id, noCalls: true });
  }

  timeline.sort((a, b) => time(a.at) - time(b.at) || a.seq - b.seq);
  return {
    nodes,
    edges,
    timeline,
    notes: {
      dropped: history.sessions.some((s) => s.dropped),
      empty: nodes.length === 0,
      startedAt: history.started_at,
    },
  };
}
