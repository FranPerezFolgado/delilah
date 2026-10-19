// Client of delilahd's HTTP and WebSocket API. The UI gets all its data from here:
// no Tauri commands, so the same UI can run in another shell or a browser.

export const DAEMON_URL: string = import.meta.env.VITE_DELILAH_URL ?? "http://127.0.0.1:7420";

export type TaskStatus =
  | { type: "queued" }
  | { type: "preparing-workspace" }
  | { type: "running" }
  | { type: "waiting-for-user" }
  | { type: "paused-by-limit"; resume_at: string }
  | { type: "done" }
  | { type: "failed"; reason: string };

export interface Task {
  key: string;
  title: string;
  project: string | null;
  workflow: string;
  phase: string | null;
  status: TaskStatus;
}

export interface EventRecord {
  at: string;
  event: { type: string; [field: string]: unknown };
}

export type ProjectKind = "parent" | "repo";

export interface Project {
  name: string;
  path: string;
  kind: ProjectKind;
  workflow: string;
}

export interface Submodule {
  name: string;
  path: string;
}

export interface UsageWindow {
  used_percentage?: number;
  resets_at?: number;
}

export interface UsageReport {
  at: string;
  task: string;
  rate_limits: Record<string, UsageWindow>;
}

export interface RunRequest {
  key: string;
  title: string;
  project: string;
  submodules: string[];
  workflow?: string;
  prompt?: string;
}

export type JiraMode = "sample" | "cloud" | "data-center";

export interface JiraSettings {
  mode: JiraMode;
  site: string | null;
  email: string | null;
  jql: string;
}

export interface JiraIssue {
  key: string;
  summary: string;
  issue_type: string;
  priority: string | null;
  status: string;
  assignee: string | null;
  components: string[];
  description: string;
  url: string | null;
  /** Status of the Delilah task for this ticket, if any. */
  task_status?: string | null;
}

export interface JiraConnect {
  mode: JiraMode;
  site?: string;
  email?: string;
  token?: string;
  jql?: string;
}

export interface SubmoduleSuggestion {
  path: string;
  reason: string;
}

export type SessionKind = "claude" | "shell";
export type SessionState = "starting" | "working" | "waiting" | "paused";

/** A terminal in a task's workspace: its primary Claude, extra Claudes or shells. */
export interface Session {
  id: string;
  task: string;
  kind: SessionKind;
  label: string;
  primary: boolean;
  closed: boolean;
  /** Whether its process is running now. */
  live: boolean;
  state: SessionState | null;
}

export interface FolderInspection {
  path: string;
  name: string;
  kind: ProjectKind;
  submodules: number;
}

export class ApiError extends Error {}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(DAEMON_URL + path, {
      ...init,
      headers: init?.body ? { "content-type": "application/json" } : undefined,
    });
  } catch {
    throw new ApiError("No se puede conectar con delilahd. ¿Está arrancado?");
  }
  const body = await res.text();
  const data = body ? JSON.parse(body) : null;
  if (!res.ok) throw new ApiError(data?.error ?? `delilahd respondió ${res.status}`);
  return data as T;
}

const post = <T>(path: string, body: unknown = {}) => request<T>(path, { method: "POST", body: JSON.stringify(body) });

export const api = {
  health: () => request<{ version: string }>("/api/v1/health"),
  tasks: () => request<Task[]>("/api/v1/tasks"),
  events: (key: string) => request<EventRecord[]>(`/api/v1/tasks/${key}/events`),
  projects: () => request<Project[]>("/api/v1/projects"),
  submodules: (project: string) => request<Submodule[]>(`/api/v1/projects/${project}/submodules`),
  suggestSubmodules: (project: string, t: { key: string; title: string; components: string[]; description?: string }) =>
    post<{ submodules: SubmoduleSuggestion[]; model: string }>(`/api/v1/projects/${project}/suggest-submodules`, t),
  addProject: (p: { path: string; kind: ProjectKind; name?: string }) => post<Project>("/api/v1/projects", p),
  inspectFolder: (path: string) => post<FolderInspection>("/api/v1/projects/inspect", { path }),
  run: (req: RunRequest) => post<Task>("/api/v1/runs", req),
  resume: (key: string) => post<Task>(`/api/v1/tasks/${key}/resume`),
  done: (key: string) => post<Task>(`/api/v1/tasks/${key}/done`),
  archive: (key: string) => post<Task>(`/api/v1/tasks/${key}/archive`),
  usage: () => request<UsageReport | null>("/api/v1/usage"),
  jira: () => request<{ settings: JiraSettings | null }>("/api/v1/jira"),
  connectJira: (c: JiraConnect) =>
    request<{ account: string; settings: JiraSettings }>("/api/v1/jira", { method: "PUT", body: JSON.stringify(c) }),
  jiraIssues: () => request<JiraIssue[]>("/api/v1/jira/issues"),
  sessions: () => request<Session[]>("/api/v1/sessions"),
  preference: <T>(key: string) => request<T | null>(`/api/v1/preferences/${key}`),
  setPreference: (key: string, value: unknown) =>
    request<unknown>(`/api/v1/preferences/${key}`, { method: "PUT", body: JSON.stringify(value) }),
  openSession: (task: string, s: { kind: SessionKind; label?: string; prompt?: string }) =>
    post<Session>(`/api/v1/tasks/${task}/sessions`, s),
  closeSession: (id: string) => post<null>(`/api/v1/sessions/${id}/close`),
  reopenSession: (id: string) => post<Session>(`/api/v1/sessions/${id}/reopen`),
};

export const terminalUrl = (sessionId: string) => `${DAEMON_URL.replace(/^http/, "ws")}/api/v1/sessions/${sessionId}/terminal`;

/** What the "Revisar cambios" session is asked first. */
export const REVIEW_PROMPT =
  "Revisa los cambios de este workspace respecto a la rama de la que salió (git diff y git log). " +
  "Dime qué está bien, qué cambiarías y si falta algún test. No modifiques nada todavía.";

// Statuses whose agent may still have a live terminal.
export const isActive = (s: TaskStatus) =>
  s.type === "running" || s.type === "waiting-for-user" || s.type === "paused-by-limit" || s.type === "preparing-workspace";

export const statusLabel = (s: TaskStatus): string => {
  switch (s.type) {
    case "queued":
      return "En cola";
    case "preparing-workspace":
      return "Preparando";
    case "running":
      return "En ejecución";
    case "waiting-for-user":
      return "Te espera";
    case "paused-by-limit":
      return `Pausada hasta las ${hhmm(s.resume_at)}`;
    case "done":
      return "Hecha";
    case "failed":
      return "Fallida";
  }
};

export const hhmm = (iso: string) => new Date(iso).toLocaleTimeString("es-ES", { hour: "2-digit", minute: "2-digit" });
