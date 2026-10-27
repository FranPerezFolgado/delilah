import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ApiError, DAEMON_URL, DEFAULT_PROJECT_SETTINGS, api, taskKind, baseStaleNotices, isCheckoutDirty, onRequestResult,
  type Health, type HistoryEntry, type InboxItem, type InboxKind, type JiraIssue, type MergeMethod, type PermissionCardEntry,
  type PrCheck, type PrSnapshot, type ReviewComment, type Session, type SetupInfo, type Task,
} from "./api";

const item = (task: string, kind: InboxItem["kind"]): InboxItem =>
  ({ task, kind, title: task }) as InboxItem;

describe("baseStaleNotices", () => {
  it("keeps only the given task's base-stale items", () => {
    const inbox = [
      item("PROJ-1", "base-stale"),
      item("PROJ-2", "base-stale"),
      item("PROJ-1", "pr-failed"),
      item("PROJ-1", "waiting"),
    ];
    expect(baseStaleNotices(inbox, "PROJ-1")).toEqual([inbox[0]]);
    expect(baseStaleNotices(inbox, "PROJ-3")).toEqual([]);
  });
});

// Type-level: a sample JSON of contracts/api.md assigns to each type (tsc checks it).
describe("v2 read types", () => {
  it("accepts the contract samples", () => {
    const health: Health = { version: "0.12.0", exe: "x", exe_mtime: 0, home: "/Users/me/.delilah" };
    const session: Pick<Session, "runtime" | "effective_runtime" | "runtime_override" | "last_activity_at" | "last_message"> = {
      runtime: "pty", effective_runtime: "stream", runtime_override: null, last_activity_at: "2026-10-24T09:12:00Z", last_message: null,
    };
    const check: PrCheck = { name: "ci", outcome: "pending", url: null, started_at: "t", finished_at: null };
    const comment: ReviewComment = { author: "ana", path: null, line: 12, body: "b", at: "t" };
    const methods: MergeMethod[] = ["merge", "squash", "rebase"];
    const snap: Pick<PrSnapshot, "review_comments" | "merge_methods"> = { review_comments: [comment], merge_methods: methods };
    const kinds: InboxKind[] = ["turn-done", "pr-opened"];
    const id: InboxItem["id"] = "question:LIBRE-5:3f2a:1729760000000";
    const card: Pick<PermissionCardEntry, "decision"> = { decision: "allow_once" };
    const jira: Pick<JiraIssue, "subtask_count" | "subtasks_done"> = { subtask_count: 20, subtasks_done: null };
    const task: Pick<Task, "created_at"> = { created_at: "2026-10-24T09:12:00Z" };
    const setup: SetupInfo = {
      command: "npm ci", exit_code: 1, started_at: "t", finished_at: "t", duration_ms: 8123,
      cancelled: false, timed_out: false, tail: "x", log_bytes: 20480,
    };
    expect([health, session, check, snap, kinds, id, card, jira, task, setup]).toHaveLength(10);
  });
});

describe("v2 calls", () => {
  const calls: { url: string; init?: RequestInit }[] = [];
  const reply = (status: number, body: string) =>
    vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({ url, init });
      return new Response(status === 204 ? null : body, { status });
    }));
  beforeEach(() => { calls.length = 0; });
  afterEach(() => vi.unstubAllGlobals());
  const last = () => calls[calls.length - 1];
  const path = () => last().url.replace(DAEMON_URL, "");

  it("reopen", async () => {
    reply(200, "{}");
    await api.reopen("K-1");
    expect(path()).toBe("/api/v1/tasks/K-1/reopen");
    expect(last().init?.method).toBe("POST");
  });
  it("mergePr with and without a method", async () => {
    reply(200, "{}");
    await api.mergePr("K-1", 7, "squash");
    expect(path()).toBe("/api/v1/tasks/K-1/prs/7/merge");
    expect(last().init?.method).toBe("POST");
    expect(JSON.parse(last().init?.body as string)).toEqual({ method: "squash" });
    await api.mergePr("K-1", 7);
    expect(JSON.parse(last().init?.body as string)).toEqual({});
  });
  it("setupInfo and setupLogUrl", async () => {
    reply(200, "{}");
    await api.setupInfo("K-1");
    expect(path()).toBe("/api/v1/tasks/K-1/setup");
    expect(last().init?.method).toBeUndefined();
    expect(api.setupLogUrl("K-1")).toBe(`${DAEMON_URL}/api/v1/tasks/K-1/setup/log`);
  });
  it("setupLogUrl encodes the key", () => {
    expect(api.setupLogUrl("a/b c")).toBe(`${DAEMON_URL}/api/v1/tasks/a%2Fb%20c/setup/log`);
  });
  it("baseStatus reads and updateBranch posts", async () => {
    reply(200, JSON.stringify({ base: "origin/main", behind_by: 2 }));
    expect(await api.baseStatus("K-1")).toEqual({ base: "origin/main", behind_by: 2 });
    expect(path()).toBe("/api/v1/tasks/K-1/base-status");
    reply(200, JSON.stringify({ base: "origin/main", merged: 2 }));
    expect((await api.updateBranch("K-1")).merged).toBe(2);
    expect(path()).toBe("/api/v1/tasks/K-1/update-branch");
    expect(last().init?.method).toBe("POST");
  });
  it("isCheckoutDirty trusts the reason, and reads the message only without one", () => {
    expect(isCheckoutDirty(new ApiError("x", 409, false, "checkout-dirty"))).toBe(true);
    expect(isCheckoutDirty(new ApiError("has uncommitted changes", 409, false, "other"))).toBe(false);
    expect(isCheckoutDirty(new ApiError("/p has uncommitted changes"))).toBe(true);
    expect(isCheckoutDirty(new Error("boom"))).toBe(false);
  });
  it("dismissInbox encodes the id", async () => {
    reply(204, "");
    const id = "question:LIBRE-5:a/b%c d:1";
    await api.dismissInbox(id);
    expect(path()).toBe(`/api/v1/inbox/${encodeURIComponent(id)}/dismiss`);
    expect(path()).toBe("/api/v1/inbox/question%3ALIBRE-5%3Aa%2Fb%25c%20d%3A1/dismiss");
    expect(last().init?.method).toBe("POST");
  });
  it("clearInbox and restoreInbox", async () => {
    reply(200, '{"ids":["a"]}');
    expect(await api.clearInbox()).toEqual({ ids: ["a"] });
    expect(path()).toBe("/api/v1/inbox/clear");
    expect(last().init?.method).toBe("POST");
    reply(204, "");
    await api.restoreInbox(["a", "b"]);
    expect(path()).toBe("/api/v1/inbox/restore");
    expect(JSON.parse(last().init?.body as string)).toEqual({ ids: ["a", "b"] });
  });
  it("jiraSubtasks", async () => {
    reply(200, "[]");
    await api.jiraSubtasks("K-1");
    expect(path()).toBe("/api/v1/jira/issues/K-1/subtasks");
  });
  it("setSessionRuntime", async () => {
    reply(204, "");
    await api.setSessionRuntime("s1", "chat");
    expect(path()).toBe("/api/v1/sessions/s1/runtime");
    expect(last().init?.method).toBe("POST");
    expect(JSON.parse(last().init?.body as string)).toEqual({ mode: "chat" });
  });
  it("onRequestResult reports responses and network failures, and unsubscribes", async () => {
    const seen: boolean[] = [];
    const off = onRequestResult((ok, at) => { seen.push(ok); expect(typeof at).toBe("number"); });
    reply(200, "[]");
    await api.tasks();
    reply(500, '{"error":"x"}');
    await expect(api.tasks()).rejects.toThrow();
    vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("down"); }));
    await expect(api.tasks()).rejects.toThrow();
    expect(seen).toEqual([true, true, false]);
    off();
    await expect(api.tasks()).rejects.toThrow();
    expect(seen).toHaveLength(3);
  });
});

describe("activity calls", () => {
  const urls: string[] = [];
  const reply = (body: unknown) =>
    vi.stubGlobal("fetch", vi.fn(async (url: string) => {
      urls.push(url);
      return new Response(JSON.stringify(body), { status: 200 });
    }));
  beforeEach(() => { urls.length = 0; });
  afterEach(() => vi.unstubAllGlobals());
  const path = () => urls[urls.length - 1].replace(DAEMON_URL, "");

  it("activityHistory encodes the owner and adds after only when given", async () => {
    reply({ started_at: "t", sessions: [] });
    await api.activityHistory("@web");
    expect(path()).toBe("/api/v1/activity/history?owner=%40web");
    await api.activityHistory("s1", 7);
    expect(path()).toBe("/api/v1/activity/history?owner=s1&after=7");
    await api.activityHistory("s1", 0);
    expect(path()).toBe("/api/v1/activity/history?owner=s1&after=0");
  });

  it("activity defaults tokens to {} for an older daemon", async () => {
    reply({ sessions: {} });
    expect(await api.activity()).toEqual({ sessions: {}, tokens: {} });
  });

  it("history entries narrow by type", () => {
    const e: HistoryEntry = { seq: 1, at: "t", type: "tool_ended", agent: null, id: "a", failed: false };
    expect(e.type === "tool_ended" && e.failed).toBe(false);
  });
});

describe("parent tasks and subtasks", () => {
  const calls: { url: string; init?: RequestInit }[] = [];
  beforeEach(() => {
    calls.length = 0;
    vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({ url, init });
      return new Response("[]", { status: 200 });
    }));
  });
  afterEach(() => vi.unstubAllGlobals());
  const last = () => calls[calls.length - 1];
  const path = () => last().url.replace(DAEMON_URL, "");

  it("reads an absent kind as normal", () => {
    expect(taskKind({})).toBe("normal");
    expect(taskKind({ kind: "subtask" })).toBe("subtask");
    expect(DEFAULT_PROJECT_SETTINGS.parent_task_merge).toBe("merge");
  });

  it("calls the new endpoints", async () => {
    await api.subtasks("P-1");
    expect(path()).toBe("/api/v1/tasks/P-1/subtasks");
    await api.makeParentTask("P-1");
    expect([path(), last().init?.method]).toEqual(["/api/v1/tasks/P-1/make-parent-task", "POST"]);
    await api.attach("P-2", "P-1");
    expect([path(), last().init?.body]).toEqual(["/api/v1/tasks/P-2/attach", JSON.stringify({ parent_task: "P-1" })]);
    await api.pushBranch("P-1");
    expect(path()).toBe("/api/v1/tasks/P-1/push-branch");
    await api.markReady("P-1", 7);
    expect([path(), last().init?.method]).toEqual(["/api/v1/tasks/P-1/prs/7/ready", "POST"]);
  });

  it("sends the new merge and open-PR fields only when set", async () => {
    await api.mergePr("P-1", 3);
    expect(last().init?.body).toBe("{}");
    await api.mergePr("P-1", 3, "squash", true);
    expect(JSON.parse(String(last().init?.body))).toEqual({ method: "squash", confirm_live_subtasks: true });
    await api.openPr("P-1", { repo: "r", title: "t", body: "", draft: false, empty_commit: true });
    expect(JSON.parse(String(last().init?.body)).empty_commit).toBe(true);
  });
});

describe("request() error shapes", () => {
  const failWith = (status: number, body: unknown) =>
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(body), { status })));

  it("uses message and keeps the refusal reason and detail", async () => {
    failWith(409, { error: "parent-task-branch-missing", message: "La rama del padre no existe", key: "P-1", branch: "b" });
    const e = await api.tasks().catch((x) => x);
    expect(e).toBeInstanceOf(ApiError);
    expect(e.message).toBe("La rama del padre no existe");
    expect(e.reason).toBe("parent-task-branch-missing");
    expect(e.detail).toMatchObject({ key: "P-1", branch: "b" });
  });

  it("keeps plain {error} bodies and an explicit reason as before", async () => {
    failWith(400, { error: "boom" });
    const a = await api.tasks().catch((x) => x);
    expect([a.message, a.reason]).toEqual(["boom", undefined]);
    failWith(409, { error: "x", reason: "has-pr" });
    expect((await api.tasks().catch((x) => x)).reason).toBe("has-pr");
  });
});
