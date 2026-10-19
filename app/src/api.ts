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
  addProject: (p: { path: string; kind: ProjectKind; name?: string }) => post<Project>("/api/v1/projects", p),
  run: (req: RunRequest) => post<Task>("/api/v1/runs", req),
  resume: (key: string) => post<Task>(`/api/v1/tasks/${key}/resume`),
  done: (key: string) => post<Task>(`/api/v1/tasks/${key}/done`),
  archive: (key: string) => post<Task>(`/api/v1/tasks/${key}/archive`),
  usage: () => request<UsageReport | null>("/api/v1/usage"),
};

export const terminalUrl = (key: string) => `${DAEMON_URL.replace(/^http/, "ws")}/api/v1/tasks/${key}/terminal`;

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
