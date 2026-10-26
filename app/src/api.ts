// Client of delilahd's HTTP and WebSocket API. The UI gets all its data from here:
// no Tauri commands, so the same UI can run in another shell or a browser.
import { locale, t } from "./i18n";

export const DAEMON_URL: string = import.meta.env.VITE_DELILAH_URL ?? "http://127.0.0.1:7420";

export type TaskStatus =
  | { type: "queued" }
  | { type: "preparing-workspace" }
  | { type: "running" }
  | { type: "waiting-for-user" }
  | { type: "paused-by-limit"; resume_at: string }
  | { type: "done" }
  | { type: "failed"; reason: string }
  // The workspace setup failed, was cancelled or interrupted; reason is the end of its output.
  | { type: "setup-failed"; reason: string };

/** The steps of a saving-mode task, each in its own session. */
export type Step = "plan" | "implement" | "review";
export type HandoffStage = "offered" | "saving" | "starting" | "failed" | "skipped";

/** A pending move to the next step's fresh session. */
export interface Handoff {
  to: Step;
  stage: HandoffStage;
  error?: string;
}

/** Autopilot on a task: where it is, and why it stopped if paused. */
export interface Autopilot {
  state: "running" | "paused" | "done";
  stage?: "preview" | "opening_pr";
  reason?: string;
  since: string;
  nudged?: Step;
  review_session?: string;
}

/** A task's preview process (the project's preview command). */
export interface PreviewView {
  state: "starting" | "running" | "failed";
  url?: string;
  error?: string;
  started_at: string;
}

export interface Task {
  key: string;
  /** When the task was created (RFC 3339). */
  created_at: string;
  /** When a done task got there (RFC 3339); null in any other state. */
  finished_at: string | null;
  title: string;
  project: string | null;
  workflow: string;
  phase: string | null;
  status: TaskStatus;
  /** What the user keeps with the task; handed to the agent on start. */
  notes?: string | null;
  /** Backlog: where it will work once started (checkout or worktree); null if not chosen. */
  in_checkout?: boolean | null;
  /** The branch of the task's workspace; absent without a workspace (read-only). */
  branch?: string | null;
  /** Saving mode: each step runs in a fresh session. */
  saving_mode: boolean;
  /** Autopilot: null when off. */
  autopilot?: Autopilot | null;
  /** How the task's agent is shown: the chat view or its terminal. */
  view?: TaskView;
  /** Experimental: the agent runs headless (stream-json) with no terminal. */
  chat_mode?: boolean;
  /** Tools a chat-mode agent may use without asking. */
  allowed_tools?: string[];
  handoff: Handoff | null;
  /** Who set the title: the first one, the analysis's feature name, or the user. */
  title_source: "initial" | "spec" | "manual";
  /** The title before the analysis renamed the task; null once edited by hand or restored. */
  previous_title: string | null;
  /** Why the branch could not be renamed, if it could not. */
  branch_error: string | null;
}

export type TaskView = "chat" | "terminal";

export interface EventRecord {
  at: string;
  event: { type: string; [field: string]: unknown };
}

export type ProjectKind = "parent" | "repo";

export type WorkspaceMode = "checkout" | "worktree";
export type SharedMode = "copy" | "link";

export interface SharedEntry {
  path: string;
  /** null: decided by the kind of entry when it is used. */
  mode: SharedMode | null;
}

export interface ProjectSettings {
  /** null: not chosen, so the checkout. */
  workspace: WorkspaceMode | null;
  /** Branch template (needs {ticket}); null: Delilah proposes a descriptive name. */
  branch: string | null;
  shared: SharedEntry[];
  setup: string | null;
  submodules: { always: string[]; never: string[] };
}

/** Body of a 422 from the settings endpoint: one message per invalid field. */
export interface SettingsError {
  error: string;
  fields: { field: string; code: string; message: string }[];
}

/** The effective settings of a project with nothing configured. */
export const DEFAULT_PROJECT_SETTINGS: ProjectSettings = {
  workspace: null,
  branch: null,
  shared: [],
  setup: null,
  submodules: { always: [], never: [] },
};

export interface Project {
  name: string;
  path: string;
  kind: ProjectKind;
  workflow: string;
  /** Effective settings, defaults included. */
  settings: ProjectSettings;
  /** Command that serves a task's workspace as a preview; null or absent: none. */
  preview_command?: string | null;
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

/** A task saved to a project's backlog, not started. */
export interface NewBacklogTask {
  key?: string;
  title: string;
  project: string;
  notes?: string;
  /** No Jira ticket: Delilah names it LIBRE-n. */
  free?: boolean;
  /** Where it will work once started: the project's checkout or a worktree. */
  in_checkout?: boolean;
  saving_mode?: boolean;
  /** Implies saving mode. */
  autopilot?: boolean;
  view?: TaskView;
  /** Experimental chat mode: no terminal. */
  chat_mode?: boolean;
}

/** What can change in a backlog task before it starts. */
export interface BacklogChange {
  title?: string;
  /** An empty string clears them. */
  notes?: string;
  project?: string;
  in_checkout?: boolean;
  saving_mode?: boolean;
  /** Implies saving mode. */
  autopilot?: boolean;
  view?: TaskView;
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
  /** Saving mode; absent: the `saving-mode` preference (kept for backlog tasks). */
  saving_mode?: boolean;
  /** Implies saving mode. */
  autopilot?: boolean;
  view?: TaskView;
  /** Experimental chat mode: no terminal. */
  chat_mode?: boolean;
  /** What the agent gets with the task; for a backlog task, "" clears its notes. */
  notes?: string;
  /** A project's Claude session to carry on as the task's primary one. */
  from_session?: string;
  /** Full branch name (`feature/...`); empty or absent lets the daemon choose. */
  branch?: string;
}

export interface BranchProposal {
  branch: string | null;
  source: "claude" | "title" | "key";
}

export type JiraMode = "sample" | "cloud" | "data-center";

export interface JiraSettings {
  mode: JiraMode;
  site: string | null;
  email: string | null;
  jql: string;
}

/** The daemon's JQL when the connection saves none. */
export const DEFAULT_JQL = "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC";

/** Whose tickets: assigned to me, reported by me, or the connection's saved JQL. */
export type JiraWho = "assigned" | "reported" | "query";
export type JiraProgress = "open" | "done";
export type JiraSprintFilter = "all" | "current";

export interface JiraEpic {
  key: string;
  summary: string | null;
}

/** The story or task a sub-task hangs from. */
export interface JiraParent {
  key: string;
  summary: string | null;
}

export interface JiraSprint {
  name: string;
  /** "active", "future" or "closed". */
  state: string;
}

export interface JiraIssue {
  key: string;
  summary: string;
  issue_type: string;
  priority: string | null;
  status: string;
  /** Jira's bucket for the status: "new", "indeterminate" or "done". */
  status_category: string;
  assignee: string | null;
  reporter: string | null;
  epic: JiraEpic | null;
  subtask: boolean;
  /** The story or task above a sub-task. */
  parent: JiraParent | null;
  sprint: JiraSprint | null;
  components: string[];
  description: string;
  url: string | null;
  /** Status of the Delilah task for this ticket, if any. */
  task_status?: string | null;
  /** Children count; null for a sub-task itself. */
  subtask_count?: number | null;
  subtasks_done?: number | null;
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

export type SessionKind = "claude" | "shell" | "setup";
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
  /** An idea conversation (a project session that turns an idea into a task). */
  conversation: boolean;
  /** The saving-mode step it works on, if any. */
  step: Step | null;
  /** The live process's runtime, else the one it would launch with. "stream": chat mode, no terminal. */
  runtime?: "pty" | "stream";
  /** What the next launch uses: `runtime_override`, else the task's chat mode. */
  effective_runtime: "pty" | "stream";
  /** The stored per-session choice; null follows the task. */
  runtime_override: "pty" | "stream" | null;
  last_activity_at: string | null;
  last_message: string | null;
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

export type InboxKind =
  | "permission" | "question" | "error" | "waiting" | "review" | "handoff" | "suggest-review" | "paused" | "failed"
  | "pr-failed" | "pr-review" | "pr-autofix" | "pr-blocked" | "branch" | "autopilot" | "base-stale" | "turn-done" | "pr-opened";


/** One question of Claude's AskUserQuestion, as the agent wrote it. */
export interface AskQuestion {
  question: string;
  header?: string;
  options: { label: string; description?: string; preview?: string }[];
  multiSelect?: boolean;
}

/** One row of a diff, as the daemon sends it (the Changes view's shape). */
export interface DiffRow {
  kind: "add" | "del" | "ctx" | "hunk";
  old: number | null;
  new: number | null;
  text: string;
}

/** A tool call of the agent, with its result once it answered. */
export interface ToolEntry {
  id: string;
  at: string;
  kind: "tool";
  tool: string;
  tool_use_id: string;
  summary: string;
  input: Record<string, unknown>;
  /** null while the tool has not answered. */
  result: string | null;
  is_error: boolean;
  truncated: boolean;
  diff?: DiffRow[];
  path?: string;
  /** The assistant message the call came from. */
  message_id?: string;
}

/** One entry of a conversation. */
export type ConversationEntry =
  | { id: string; at: string; kind: "user"; text: string; state: "queued" | "sent" | "delivered"; from: "view" | "terminal" }
  | { id: string; at: string; kind: "claude"; text: string; message_id?: string }
  | ToolEntry
  | { id: string; at: string; kind: "marker"; marker: "compacted" | "stopped" }
  | QuestionCardEntry
  | PermissionCardEntry;

interface CardBase {
  id: string;
  at: string;
  kind: "card";
  ask: string;
  outcome: "pending" | "answered" | "terminal" | "withdrawn";
  tool_use_id?: string;
}

/** A question with options (or a proposal); the default card when ask_kind is absent. */
export interface QuestionCardEntry extends CardBase {
  ask_kind?: "question";
  questions: AskQuestion[];
  proposal: { title: string; brief: string } | null;
  answer: Record<string, string> | null;
  task: string | null;
  error: string | null;
}

/** A permission ask of the agent; gone from the conversation once answered. */
export interface PermissionCardEntry extends CardBase {
  ask_kind: "permission";
  tool: string;
  summary: string;
  suggestion: unknown;
  /** The exact rule "allow always" would remember; null when there is none. */
  rule: string | null;
  /** How it was answered; null while pending or when it ended without an answer. */
  decision?: PermissionDecision | null;
}

export type PermissionDecision = "allow_once" | "allow_always" | "deny";

/** A permission ask as the inbox carries it. */
export interface PermissionAsk {
  id: string;
  tool: string;
  summary: string;
  suggestion: unknown;
  rule: string | null;
}

export interface Conversation {
  entries: ConversationEntry[];
  /** Resume point for the next call. */
  cursor: string;
  complete: boolean;
  transcript: "present" | "pending" | "missing";
  activity: string | null;
  needs_terminal: boolean;
  runtime: "pty" | "stream";
}

/** The untruncated input and result of one tool call. */
export interface ToolDetail {
  input: Record<string, unknown>;
  result: string | null;
  is_error: boolean;
  truncated: boolean;
}

/** Something that waits on you, from any task. */
export interface InboxItem {
  /** `<kind>:<task>:<source>:<since ms>`; percent-encode it in a URL (`dismissInbox` does). */
  id: string;
  kind: InboxKind;
  task: string;
  title: string;
  session: { id: string; label: string; primary: boolean; live: boolean } | null;
  text: string | null;
  since: string | null;
  /** A question with options, answered from Delilah. */
  ask?: { id: string; questions: AskQuestion[]; proposal?: { title: string; brief: string } | null };
  /** The PR notice behind a pr-* item. */
  pr?: { notice: number; url: string; repo: string };
  /** The permission asked, for a permission item. */
  permission?: PermissionAsk;
  /** The pr_notices row behind a base-stale item: dismiss it with `dismissNotice`. */
  notice?: number;
}

/** Where a session's questions with options sit, next to its terminal. */
export type AskPosition = "top" | "right" | "bottom";
export const ASK_POSITIONS: AskPosition[] = ["top", "right", "bottom"];

/** A saved position, or the default (right) when there is none or it is unknown. */
export function parseAskPosition(v: unknown): AskPosition {
  return ASK_POSITIONS.includes(v as AskPosition) ? (v as AskPosition) : "right";
}

/** The kinds that block an agent until you act: they count in the badge. */
export const BLOCKING: InboxKind[] = ["permission", "question", "error", "waiting", "pr-failed", "pr-review", "pr-autofix", "pr-blocked"];

/** The stale-base warnings of one task, for its screen's banner. */
export const baseStaleNotices = (inbox: InboxItem[], task: string): InboxItem[] =>
  inbox.filter((i) => i.task === task && i.kind === "base-stale");

export interface FolderInspection {
  path: string;
  name: string;
  kind: ProjectKind;
  submodules: number;
}

export interface PrCheck {
  name: string;
  outcome: "pending" | "success" | "failure" | "neutral";
  url: string | null;
  started_at?: string | null;
  finished_at?: string | null;
}
export type MergeMethod = "merge" | "squash" | "rebase";
export interface ReviewComment { author: string; path: string | null; line: number | null; body: string; at: string }
export interface PrSnapshot {
  state: "open" | "merged" | "closed";
  draft: boolean;
  title: string;
  head: string;
  base: string;
  checks: PrCheck[];
  ci: "pending" | "success" | "failure" | "none";
  review_decision: "approved" | "changes-requested" | "review-required" | null;
  comments: number;
  /** The newest 50. */
  review_comments: ReviewComment[];
  /** What the repository allows; [] when unknown. */
  merge_methods: MergeMethod[];
}
export interface PullRequest {
  id: number;
  repo: string;
  host: string;
  owner_name: string;
  number: number;
  url: string;
  linked_by: "detected" | "opened" | "pasted";
  checked_at: string | null;
  error: string | null;
  snapshot: PrSnapshot | null;
}
export interface TaskPrs { account: string | null; problem: { kind: string; message: string } | null; prs: PullRequest[] }
/** What `Abrir PR` starts from: the branch's last commit as title and body. */
export interface PrProposal { repo: string; title: string; body: string; base: string; needs_push: boolean; repos: string[] }
/** One task's PRs for badges: the worst of each field when it has several. */
export interface PrSummary {
  task: string;
  /** The PR the badge stands for. */
  number: number;
  state: PrSnapshot["state"];
  draft: boolean;
  ci: PrSnapshot["ci"];
  review_decision: PrSnapshot["review_decision"];
  count: number;
}

/** The last run of a task's workspace setup (`GET …/setup`). */
export interface SetupInfo {
  command: string;
  exit_code: number | null;
  started_at: string;
  finished_at: string;
  duration_ms: number;
  cancelled: boolean;
  timed_out: boolean;
  tail: string;
  log_bytes: number;
}

/** How far a task's branch is from its base (`GET …/base-status`). */
export interface BaseStatus {
  /** The ref compared with (`origin/main`); null when none could be resolved. */
  base: string | null;
  /** Commits of the base the branch does not have. */
  behind_by: number;
}
/** What `POST …/update-branch` did: the commits it merged in (0: up to date). */
export interface BranchUpdate { base: string | null; merged: number }

export interface Health { version: string; exe?: string | null; exe_mtime?: number | null; home: string }

/** The `reason` of the 409 for a start in a checkout that has uncommitted changes. */
export const CHECKOUT_DIRTY = "checkout-dirty";

/** Whether a start was refused because the project's checkout has uncommitted changes. */
export function isCheckoutDirty(e: unknown): boolean {
  if (e instanceof ApiError && e.reason) return e.reason === CHECKOUT_DIRTY;
  // An older daemon sends no reason: read the message.
  const message = e instanceof Error ? e.message : "";
  return /uncommitted|must be clean|not clean/i.test(message) || (/checkout/i.test(message) && /(cambios|changes|dirty)/i.test(message));
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status?: number,
    /** A delete refused because work in the task's worktree would be lost. */
    readonly unpushed = false,
    /** Why a PR can't be opened (`no-commits`, `has-pr`, `no-workspace`). */
    readonly reason?: string,
    /** When a paused task resumes (a 409 that is a pause, not any other conflict). */
    readonly resumeAt?: string,
    /** A 422 from the settings endpoint: one message per invalid field. */
    readonly fields: SettingsError["fields"] = [],
  ) {
    super(message);
  }
}

type RequestObserver = (ok: boolean, at: number) => void;
const observers = new Set<RequestObserver>();

/** Watches every request: `ok` is false on a network failure. Returns the unsubscribe. */
export function onRequestResult(cb: RequestObserver): () => void {
  observers.add(cb);
  return () => void observers.delete(cb);
}
const notify = (ok: boolean) => observers.forEach((cb) => cb(ok, Date.now()));

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(DAEMON_URL + path, {
      ...init,
      headers: init?.body ? { "content-type": "application/json" } : undefined,
    });
  } catch {
    notify(false);
    throw new ApiError(t("No se puede conectar con delilahd. ¿Está arrancado?"));
  }
  notify(true);
  const body = await res.text();
  const data = body ? JSON.parse(body) : null;
  if (!res.ok)
    throw new ApiError(data?.error ?? t("delilahd respondió {status}", { status: res.status }), res.status, data?.unpushed === true, data?.reason, data?.resume_at, Array.isArray(data?.fields) ? data.fields : []);
  return data as T;
}

const post = <T>(path: string, body: unknown = {}) => request<T>(path, { method: "POST", body: JSON.stringify(body) });

export const api = {
  health: () => request<Health>("/api/v1/health"),
  reopen: (key: string) => post<Task>(`/api/v1/tasks/${key}/reopen`),
  /** Merges a task's PR; `method` omitted: the daemon picks. */
  mergePr: (key: string, id: number, method?: MergeMethod) =>
    post<PullRequest>(`/api/v1/tasks/${key}/prs/${id}/merge`, method ? { method } : {}),
  baseStatus: (key: string) => request<BaseStatus>(`/api/v1/tasks/${key}/base-status`),
  /** Merges the base into the task's branch; 409 with a `reason` (`workspace-dirty`, `update-conflict`…) when refused. */
  updateBranch: (key: string) => post<BranchUpdate>(`/api/v1/tasks/${key}/update-branch`),
  setupInfo: (key: string) => request<SetupInfo>(`/api/v1/tasks/${key}/setup`),
  /** The whole setup log as text/plain, to open in a new tab or window. */
  setupLogUrl: (key: string) => `${DAEMON_URL}/api/v1/tasks/${encodeURIComponent(key)}/setup/log`,
  dismissInbox: (id: string) => post<null>(`/api/v1/inbox/${encodeURIComponent(id)}/dismiss`),
  /** Dismisses every non-blocking item; returns their ids (for undo). */
  clearInbox: () => post<{ ids: string[] }>("/api/v1/inbox/clear"),
  restoreInbox: (ids: string[]) => post<null>("/api/v1/inbox/restore", { ids }),
  jiraSubtasks: (key: string) => request<JiraIssue[]>(`/api/v1/jira/issues/${key}/subtasks`),
  setSessionRuntime: (id: string, mode: "chat" | "terminal") => post<null>(`/api/v1/sessions/${id}/runtime`, { mode }),
  tasks: () => request<Task[]>("/api/v1/tasks"),
  events: (key: string) => request<EventRecord[]>(`/api/v1/tasks/${key}/events`),
  projects: () => request<Project[]>("/api/v1/projects"),
  /** `all` includes the `never` submodules (the settings screen edits the rules). */
  submodules: (project: string, opts: { all?: boolean } = {}) =>
    request<Submodule[]>(`/api/v1/projects/${project}/submodules${opts.all ? "?all=true" : ""}`),
  suggestSubmodules: (project: string, t: { key: string; title: string; components: string[]; description?: string }) =>
    post<{ submodules: SubmoduleSuggestion[]; model: string }>(`/api/v1/projects/${project}/suggest-submodules`, t),
  proposeBranch: (
    project: string,
    t: { key?: string; title: string; notes?: string; free: boolean; submodules: string[] },
  ) => post<BranchProposal>(`/api/v1/projects/${project}/branch-name`, t),
  addProject: (p: { path: string; kind: ProjectKind; name?: string }) => post<Project>("/api/v1/projects", p),
  inspectFolder: (path: string) => post<FolderInspection>("/api/v1/projects/inspect", { path }),
  updateProject: (name: string, change: { name?: string; path?: string; kind?: ProjectKind; workflow?: string; preview_command?: string | null }) =>
    request<Project>(`/api/v1/projects/${name}`, { method: "PATCH", body: JSON.stringify(change) }),
  /** Saves the whole settings; a 422 throws an ApiError with `fields`. */
  updateProjectSettings: (name: string, settings: ProjectSettings) =>
    request<Project>(`/api/v1/projects/${name}/settings`, { method: "PUT", body: JSON.stringify(settings) }),
  deleteProject: (name: string) => request<null>(`/api/v1/projects/${name}`, { method: "DELETE" }),
  run: (req: RunRequest) => post<Task>("/api/v1/runs", req),
  createTask: (task: NewBacklogTask) => post<Task>("/api/v1/tasks", task),
  updateTask: (key: string, change: BacklogChange) =>
    request<Task>(`/api/v1/tasks/${key}`, { method: "PATCH", body: JSON.stringify(change) }),
  /** For good: stops its agent and releases its workspace (the branch stays). */
  deleteTask: (key: string, force = false) =>
    request<null>(`/api/v1/tasks/${key}${force ? "?force=true" : ""}`, { method: "DELETE" }),
  resume: (key: string) => post<Task>(`/api/v1/tasks/${key}/resume`),
  setupRetry: (key: string) => post<Task>(`/api/v1/tasks/${key}/setup/retry`),
  setupSkip: (key: string) => post<Task>(`/api/v1/tasks/${key}/setup/skip`),
  setupCancel: (key: string) => post<Task>(`/api/v1/tasks/${key}/setup/cancel`),
  done: (key: string) => post<Task>(`/api/v1/tasks/${key}/done`),
  /** Retries applying the branch derived from the spec; branch_error says if it failed again. */
  retryBranch: (key: string) => post<Task>(`/api/v1/tasks/${key}/branch`),
  archive: (key: string, force = false) => post<Task>(`/api/v1/tasks/${key}/archive${force ? "?force=true" : ""}`),
  /** Moves the task to a phase; only the user can move it back. */
  setPhase: (key: string, phase: SpecPhase) => post<Task>(`/api/v1/tasks/${key}/phase`, { phase }),
  /** The task's preview, or null when it has none. */
  getPreview: (key: string) => request<PreviewView | null>(`/api/v1/tasks/${key}/preview`),
  /** Starts it. 409 `no-preview-command` / `no-workspace` / `running`; 422 when it can't be spawned. */
  startPreview: (key: string) => post<PreviewView>(`/api/v1/tasks/${key}/preview`),
  stopPreview: (key: string) => request<null>(`/api/v1/tasks/${key}/preview`, { method: "DELETE" }),
  setSavingMode: (key: string, on: boolean) => post<Task>(`/api/v1/tasks/${key}/saving-mode`, { on }),
  /** Moves the task between chat mode and terminal; its sessions resume in the new runtime. */
  setRuntime: (key: string, mode: "chat" | "terminal") => post<null>(`/api/v1/tasks/${key}/runtime`, { mode }),
  setAutopilot: (key: string, action: "on" | "off" | "resume") => post<Task>(`/api/v1/tasks/${key}/autopilot`, { action }),
  /** Answers the offer to continue in a fresh session, or retries a failed one. */
  handoff: (key: string, action: "confirm" | "decline" | "retry") => post<Task>(`/api/v1/tasks/${key}/handoff`, { action }),
  dismissReviewHint: (key: string) => post<null>(`/api/v1/tasks/${key}/review-hint/dismiss`),
  usage: () => request<UsageReport | null>("/api/v1/usage"),
  jira: () => request<{ settings: JiraSettings | null }>("/api/v1/jira"),
  connectJira: (c: JiraConnect) =>
    request<{ account: string; settings: JiraSettings }>("/api/v1/jira", { method: "PUT", body: JSON.stringify(c) }),
  jiraIssues: (who: JiraWho, progress: JiraProgress, sprint: JiraSprintFilter) =>
    request<JiraIssue[]>(`/api/v1/jira/issues?who=${who}&progress=${progress}&sprint=${sprint}`),
  sessions: () => request<Session[]>("/api/v1/sessions"),
  preference: <T>(key: string) => request<T | null>(`/api/v1/preferences/${key}`),
  setPreference: (key: string, value: unknown) =>
    request<unknown>(`/api/v1/preferences/${key}`, { method: "PUT", body: JSON.stringify(value) }),
  openSession: (task: string, s: { kind: SessionKind; label?: string; prompt?: string }) =>
    post<Session>(`/api/v1/tasks/${task}/sessions`, s),
  closeSession: (id: string) => post<null>(`/api/v1/sessions/${id}/close`),
  reopenSession: (id: string) => post<Session>(`/api/v1/sessions/${id}/reopen`),
  openProjectSession: (project: string, s: { kind: SessionKind; label?: string; prompt?: string; read_only?: boolean; conversation?: boolean }) =>
    post<Session>(`/api/v1/projects/${project}/sessions`, s),
  allowEdits: (id: string) => post<Session>(`/api/v1/sessions/${id}/allow-edits`),
  taskPrs: (key: string) => request<TaskPrs>(`/api/v1/tasks/${key}/prs`),
  prSummaries: () => request<PrSummary[]>("/api/v1/prs"),
  forwardNotice: (id: number) => post<null>(`/api/v1/pr-notices/${id}/forward`),
  dismissNotice: (id: number) => post<null>(`/api/v1/pr-notices/${id}/dismiss`),
  refreshPrs: (key: string) => post<null>(`/api/v1/tasks/${key}/prs/refresh`),
  linkPr: (key: string, url: string) => post<PullRequest>(`/api/v1/tasks/${key}/prs/link`, { url }),
  prProposal: (key: string, repo = ".") =>
    request<PrProposal>(`/api/v1/tasks/${key}/prs/proposal?repo=${encodeURIComponent(repo)}`),
  openPr: (key: string, req: { repo: string; title: string; body: string; draft: boolean }) =>
    post<PullRequest>(`/api/v1/tasks/${key}/prs`, req),
  unlinkPr: (key: string, id: number) => request<null>(`/api/v1/tasks/${key}/prs/${id}`, { method: "DELETE" }),
  specs: () => request<Record<string, SpecSummary>>("/api/v1/specs"),
  inbox: () => request<InboxItem[]>("/api/v1/inbox"),
  reply: (sessionId: string, text: string) => post<null>(`/api/v1/sessions/${sessionId}/reply`, { text }),
  answerAsk: (id: string, answers: Record<string, string>, launch?: "autopilot") =>
    post<{ task: string | null; error?: string }>(`/api/v1/asks/${id}/answer`, launch ? { answers, launch } : { answers }),
  answerPermission: (sessionId: string, ask: string, decision: PermissionDecision, reason?: string) =>
    post<null>(`/api/v1/sessions/${sessionId}/permissions/${encodeURIComponent(ask)}`, reason?.trim() ? { decision, reason: reason.trim() } : { decision }),
  conversation: (id: string, cursor?: string) =>
    request<Conversation>(`/api/v1/sessions/${id}/conversation${cursor ? `?${new URLSearchParams({ cursor })}` : ""}`),
  toolDetail: (id: string, toolUseId: string) =>
    request<ToolDetail>(`/api/v1/sessions/${id}/conversation/tools/${encodeURIComponent(toolUseId)}`),
  sendMessage: (id: string, text: string) => post<ConversationEntry>(`/api/v1/sessions/${id}/messages`, { text }),
  removeMessage: (id: string, entry: string) =>
    request<null>(`/api/v1/sessions/${id}/messages/${entry}`, { method: "DELETE" }),
  stopSession: (id: string) => post<null>(`/api/v1/sessions/${id}/stop`),
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
    if (res.status === 409) throw new ConflictError(data?.error ?? t("El archivo ha cambiado en disco"));
    if (!res.ok) throw new ApiError(data?.error ?? t("delilahd respondió {status}", { status: res.status }));
    return data as FileContent;
  },
  /** A workspace image for a Markdown preview, as a `data:` URL (the only kind the CSP
   *  lets an `<img>` load from the app, besides `https:`). */
  image: async (key: string, path: string): Promise<string> => {
    const res = await fetch(`${DAEMON_URL}/api/v1/tasks/${key}/image?${new URLSearchParams({ path })}`);
    if (!res.ok) {
      const data = await res.json().catch(() => null);
      throw new ApiError(data?.error ?? t("delilahd respondió {status}", { status: res.status }));
    }
    const blob = await res.blob();
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(blob);
    });
  },
  diff: async (key: string, repo: string, path: string) => {
    const q = new URLSearchParams({ repo, path });
    const res = await fetch(`${DAEMON_URL}/api/v1/tasks/${key}/changes/diff?${q}`);
    if (!res.ok) throw new ApiError(t("No se pudo leer el diff de {path}", { path }));
    return res.text();
  },
  spec: (key: string) => request<SpecProgress | null>(`/api/v1/tasks/${key}/spec`),
  specFile: async (key: string, name: string) => {
    const res = await fetch(`${DAEMON_URL}/api/v1/tasks/${key}/spec/files/${encodeURIComponent(name)}`);
    if (!res.ok) throw new ApiError(t("No se pudo leer {name}", { name }));
    return res.text();
  },
};

export const terminalUrl = (sessionId: string) => `${DAEMON_URL.replace(/^http/, "ws")}/api/v1/sessions/${sessionId}/terminal`;

/** What the "Revisar cambios" session is asked first. */
export const reviewPrompt = () =>
  t(
    "Revisa los cambios de este workspace respecto a la rama de la que salió (git diff y git log). Dime qué está bien, qué cambiarías y si falta algún test. No modifiques nada todavía.",
  );

// Statuses whose agent may still have a live terminal.
export const isActive = (s: TaskStatus) =>
  s.type === "running" || s.type === "waiting-for-user" || s.type === "paused-by-limit" || s.type === "preparing-workspace";

export const statusLabel = (s: TaskStatus): string => {
  switch (s.type) {
    case "queued":
      return t("Backlog");
    case "preparing-workspace":
      return t("Preparando");
    case "running":
      return t("En ejecución");
    case "waiting-for-user":
      return t("Te espera");
    case "paused-by-limit":
      return t("Pausada hasta las {time}", { time: hhmm(s.resume_at) });
    case "done":
      return t("Hecha");
    case "failed":
      return t("Fallida");
    case "setup-failed":
      return t("Setup fallido");
  }
};

export const hhmm = (iso: string) => new Date(iso).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });

/** The live text of a chat-mode session: deltas of the message being written, then `done`. */
export const liveSocket = (sessionId: string) => new WebSocket(`${DAEMON_URL.replace(/^http/, "ws")}/api/v1/sessions/${sessionId}/live`);
