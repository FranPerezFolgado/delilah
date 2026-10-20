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
  /** No Jira ticket: Delilah names it LIBRE-n. */
  free?: boolean;
  /** Work in the project's own checkout instead of a worktree. */
  in_checkout?: boolean;
  /** A project's Claude session to carry on as the task's primary one. */
  from_session?: string;
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
  /** Claude in plan mode: reads and explains, does not edit. */
  read_only: boolean;
  state: SessionState | null;
}

export type SpecPhase = "specify" | "plan" | "implement" | "review";

/** What the board shows of a task's SpecKit spec. */
export interface SpecSummary {
  phase: SpecPhase;
  /** The spec folder, relative to the workspace; null until the agent creates it. */
  dir: string | null;
  done: number;
  total: number;
  /** The first item of tasks.md left to do. */
  current: string | null;
}

export interface SpecItem {
  id: string | null;
  text: string;
  done: boolean;
  parallel: boolean;
  story: string | null;
}

export interface SpecProgress extends SpecSummary {
  files: string[];
  groups: { title: string; items: SpecItem[] }[];
}

export type ChangeKind = "added" | "modified" | "deleted" | "renamed" | "untracked";

export interface FileChange {
  path: string;
  old_path: string | null;
  kind: ChangeKind;
  /** Null for binary files. */
  additions: number | null;
  deletions: number | null;
}

/** What the task's branch changed in one repo of its workspace. */
export interface RepoChanges {
  /** Submodule path, or "." for a plain repo. */
  repo: string;
  base: string | null;
  commits: number;
  files: FileChange[];
}

export interface TreeEntry {
  name: string;
  dir: boolean;
}

/** A workspace file as the editor gets it. */
export interface FileContent {
  path: string;
  /** Null for binary files or files too big to edit. */
  text: string | null;
  /** Changes whenever the file does; a save must send the one it started from. */
  version: string;
  size: number;
}

/** The file changed on disk (usually the agent) since the editor loaded it. */
export class ConflictError extends Error {}

export type InboxKind = "permission" | "question" | "error" | "waiting" | "review" | "paused" | "failed";

/** One question of Claude's AskUserQuestion, as the agent wrote it. */
export interface AskQuestion {
  question: string;
  header?: string;
  options: { label: string; description?: string }[];
  multiSelect?: boolean;
}

/** Something that waits on you, from any task. */
export interface InboxItem {
  kind: InboxKind;
  task: string;
  title: string;
  session: { id: string; label: string; primary: boolean; live: boolean } | null;
  text: string | null;
  since: string | null;
  /** A question with options, answered from Delilah. */
  ask?: { id: string; questions: AskQuestion[] };
}

/** The kinds that block an agent until you act: they count in the badge. */
export const BLOCKING: InboxKind[] = ["permission", "question", "error", "waiting"];

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
  openProjectSession: (project: string, s: { kind: SessionKind; label?: string; prompt?: string; read_only?: boolean }) =>
    post<Session>(`/api/v1/projects/${project}/sessions`, s),
  allowEdits: (id: string) => post<Session>(`/api/v1/sessions/${id}/allow-edits`),
  specs: () => request<Record<string, SpecSummary>>("/api/v1/specs"),
  inbox: () => request<InboxItem[]>("/api/v1/inbox"),
  reply: (sessionId: string, text: string) => post<null>(`/api/v1/sessions/${sessionId}/reply`, { text }),
  answerAsk: (id: string, answers: Record<string, string>) => post<null>(`/api/v1/asks/${id}/answer`, { answers }),
  releaseAsk: (id: string) => post<null>(`/api/v1/asks/${id}/release`),
  changes: (key: string) => request<RepoChanges[]>(`/api/v1/tasks/${key}/changes`),
  files: (key: string) => request<string[]>(`/api/v1/tasks/${key}/files`),
  tree: (key: string, path: string) => request<TreeEntry[]>(`/api/v1/tasks/${key}/tree?${new URLSearchParams({ path })}`),
  file: (key: string, path: string) => request<FileContent>(`/api/v1/tasks/${key}/file?${new URLSearchParams({ path })}`),
  saveFile: async (key: string, path: string, text: string, version: string) => {
    const res = await fetch(`${DAEMON_URL}/api/v1/tasks/${key}/file?${new URLSearchParams({ path })}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ text, version }),
    });
    const data = await res.json().catch(() => null);
    if (res.status === 409) throw new ConflictError(data?.error ?? "El archivo ha cambiado en disco");
    if (!res.ok) throw new ApiError(data?.error ?? `delilahd respondió ${res.status}`);
    return data as FileContent;
  },
  diff: async (key: string, repo: string, path: string) => {
    const q = new URLSearchParams({ repo, path });
    const res = await fetch(`${DAEMON_URL}/api/v1/tasks/${key}/changes/diff?${q}`);
    if (!res.ok) throw new ApiError(`No se pudo leer el diff de ${path}`);
    return res.text();
  },
  spec: (key: string) => request<SpecProgress | null>(`/api/v1/tasks/${key}/spec`),
  specFile: async (key: string, name: string) => {
    const res = await fetch(`${DAEMON_URL}/api/v1/tasks/${key}/spec/files/${encodeURIComponent(name)}`);
    if (!res.ok) throw new ApiError(`No se pudo leer ${name}`);
    return res.text();
  },
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

/** "hace 5 min", "hace 2 h", "ayer"… */
export function ago(iso: string): string {
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return "ahora";
  if (s < 3600) return `hace ${Math.floor(s / 60)} min`;
  if (s < 86400) return `hace ${Math.floor(s / 3600)} h`;
  return s < 172800 ? "ayer" : `hace ${Math.floor(s / 86400)} días`;
}

export const hhmm = (iso: string) => new Date(iso).toLocaleTimeString("es-ES", { hour: "2-digit", minute: "2-digit" });
