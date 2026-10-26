// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor, within } from "@testing-library/react";
import { api, ApiError, type PullRequest, type Task, type TaskPrs } from "../api";
import * as platform from "../platform";
import { PrView } from "./PrView";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const pr: PullRequest = {
  id: 7, repo: "api", host: "github.com", owner_name: "me/repo", number: 42, url: "https://github.com/me/repo/pull/42",
  linked_by: "detected", checked_at: new Date().toISOString(), error: null,
  snapshot: {
    review_comments: [], merge_methods: [], state: "open", draft: false, title: "Reintentos", head: "abc", base: "main",
    checks: [{ name: "test", outcome: "failure", url: "https://ci/1" }],
    ci: "failure", review_decision: "changes-requested", comments: 3,
  },
};
const data = (over: Partial<TaskPrs>): TaskPrs => ({ account: "me", problem: null, prs: [pr], ...over });

describe("PrView", () => {
  it("shows the PR, its checks, review and repo", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
    const { findByText, container } = render(<PrView owner="PROJ-1" />);
    await findByText("Reintentos");
    expect(container.querySelector(".pr-chip")?.textContent).toBe("#42Checks failing");
    expect(container.textContent).toContain("Changes requested");
    expect(container.textContent).toContain("into main");
    expect(container.textContent).toContain("test");
    expect(container.textContent).toContain("api");
    expect(container.textContent).toContain("me");
  });

  it("explains a problem once, with its fix and actions", async () => {
    const taskPrs = vi.spyOn(api, "taskPrs").mockResolvedValue(
      data({ prs: [], problem: { kind: "signed-out", message: "error: raw stderr from gh" } }),
    );
    const refresh = vi.spyOn(api, "refreshPrs").mockResolvedValue(null);
    const openTerminal = vi.fn();
    const { findByText, getByText, queryByText, container } = render(<PrView owner="PROJ-1" onOpenTerminal={openTerminal} />);
    await findByText("Run gh auth login");
    expect(container.querySelectorAll(".pr-problem").length).toBe(1);
    expect(queryByText(/raw stderr/)).toBeNull();
    fireEvent.click(getByText("Open a terminal"));
    expect(openTerminal).toHaveBeenCalled();
    fireEvent.click(getByText("Retry"));
    await waitFor(() => expect(refresh).toHaveBeenCalledWith("PROJ-1"));
    await waitFor(() => expect(taskPrs.mock.calls.length).toBeGreaterThan(1));
  });

  it("disables Open PR with the gh reason when signed out", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [], problem: { kind: "signed-out", message: "x" } }));
    vi.spyOn(api, "prProposal").mockResolvedValue({ repo: ".", title: "T", body: "", base: "main", needs_push: false, repos: ["."] });
    const { findByText } = render(<PrView owner="PROJ-1" />);
    await findByText("No PR yet");
    const btn = (await findByText("Open PR")) as HTMLButtonElement;
    await waitFor(() => expect(btn.disabled).toBe(true));
    expect(btn.title).toBe("Run gh auth login");
  });

  it("offers linking by URL when there is no PR", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [] }));
    const link = vi.spyOn(api, "linkPr").mockResolvedValue(pr);
    const { findByText, container, getByText } = render(<PrView owner="PROJ-1" />);
    await findByText("No PR yet");
    fireEvent.change(container.querySelector("input")!, { target: { value: "https://github.com/me/repo/pull/42" } });
    fireEvent.click(getByText("Link by URL"));
    await waitFor(() => expect(link).toHaveBeenCalledWith("PROJ-1", "https://github.com/me/repo/pull/42"));
  });

  it("refreshes and unlinks", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
    const refresh = vi.spyOn(api, "refreshPrs").mockResolvedValue(null);
    const unlink = vi.spyOn(api, "unlinkPr").mockResolvedValue(null);
    const { findByText, getByText } = render(<PrView owner="PROJ-1" />);
    await findByText("Reintentos");
    fireEvent.click(getByText("Reload"));
    await waitFor(() => expect(refresh).toHaveBeenCalledWith("PROJ-1"));
    fireEvent.click(getByText("Unlink"));
    await waitFor(() => expect(unlink).toHaveBeenCalledWith("PROJ-1", 7));
  });

  it("disables Open PR with the reason when there is nothing to open", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [] }));
    vi.spyOn(api, "prProposal").mockRejectedValue(new ApiError("sin commits", 409, false, "no-commits"));
    const { findByText, getByText } = render(<PrView owner="PROJ-1" />);
    await findByText("No new commits on the branch");
    expect((getByText("Open PR") as HTMLButtonElement).disabled).toBe(true);
  });

  it("enables Open PR when a proposal exists", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [] }));
    vi.spyOn(api, "prProposal").mockResolvedValue({ repo: ".", title: "T", body: "", base: "main", needs_push: false, repos: ["."] });
    const { findByText, findByDisplayValue } = render(<PrView owner="PROJ-1" />);
    fireEvent.click(await findByText("Open PR"));
    await findByDisplayValue("T");
  });

  it("groups PRs under a heading per submodule", async () => {
    const other = { ...pr, id: 8, repo: "web", number: 43, snapshot: { ...pr.snapshot!, title: "Cliente" } };
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [pr, other] }));
    const { findByText, container } = render(<PrView owner="PROJ-1" />);
    await findByText("Cliente");
    const heads = Array.from(container.querySelectorAll(".pr-group-title")).map((h) => h.textContent);
    expect(heads).toEqual(["api", "web"]);
    const groups = container.querySelectorAll(".pr-group");
    expect(groups[0].textContent).toContain("Reintentos");
    expect(groups[1].textContent).toContain("Cliente");
  });

  it("shows no heading for a single repo", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
    const { findByText, container } = render(<PrView owner="PROJ-1" />);
    await findByText("Reintentos");
    expect(container.querySelector(".pr-group-title")).toBeNull();
  });

  it("offers Open PR next to Refresh when PRs exist but a repo is free", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
    const prop = vi.spyOn(api, "prProposal").mockResolvedValue({ repo: "web", title: "T", body: "", base: "main", needs_push: false, repos: ["web", "api"] });
    const { findByText, findByDisplayValue, findByRole } = render(<PrView owner="PROJ-1" />);
    await findByText("Reintentos");
    fireEvent.click(await findByText("Open PR"));
    await findByDisplayValue("T");
    await findByRole("dialog");
    expect(prop).toHaveBeenCalledWith("PROJ-1");
  });

  it("offers no Open PR when every repo already has one", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
    const prop = vi.spyOn(api, "prProposal").mockRejectedValue(new ApiError("ya", 409, false, "has-pr"));
    const { findByText, queryByText } = render(<PrView owner="PROJ-1" />);
    await findByText("Reintentos");
    await waitFor(() => expect(prop).toHaveBeenCalled());
    expect(queryByText("Open PR")).toBeNull();
  });

  const open = (over: Partial<NonNullable<PullRequest["snapshot"]>>): PullRequest => ({
    ...pr,
    snapshot: { ...pr.snapshot!, ci: "success", checks: [], ...over },
  });
  const prPending = () => vi.spyOn(api, "prProposal").mockRejectedValue(new ApiError("ya", 409, false, "has-pr"));

  it("lists checks with durations, running ones neutral, and review comments", async () => {
    prPending();
    const started = new Date(Date.now() - 90_000).toISOString();
    const done = open({
      checks: [
        { name: "build", outcome: "success", url: null, started_at: "2026-01-01T10:00:00Z", finished_at: "2026-01-01T10:01:12Z" },
        { name: "lint", outcome: "pending", url: null, started_at: started, finished_at: null },
        { name: "e2e", outcome: "failure", url: null },
      ],
      review_comments: [{ author: "ana", path: "src/a.rs", line: 7, body: "Why this?", at: new Date().toISOString() }],
    });
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [done] }));
    const { findByRole, container } = render(<PrView owner="PROJ-1" />);
    const items = within(await findByRole("list", { name: "Checks" })).getAllByRole("listitem");
    expect(items.map((li) => li.textContent)).toEqual([
      "buildpassed1 min 12 s",
      expect.stringMatching(/^lintin progress1 min 3\d s$/),
      "e2efailed",
    ]);
    expect(items[1].querySelector(".pr-ci-pending")).not.toBeNull();
    expect(items[2].querySelector(".pr-ci-failure")).not.toBeNull();
    const comments = container.querySelector(".pr-comments")!;
    expect(comments.textContent).toContain("ana");
    expect(comments.textContent).toContain("src/a.rs:7");
    expect(comments.textContent).toContain("Why this?");
  });

  it("opens the PR on GitHub", async () => {
    prPending();
    const ext = vi.spyOn(platform, "openExternal").mockResolvedValue(undefined as never);
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [open({ merge_methods: ["squash"] })] }));
    const { findByText } = render(<PrView owner="PROJ-1" />);
    fireEvent.click(await findByText("Open in GitHub"));
    expect(ext).toHaveBeenCalledWith(pr.url);
  });

  it("merges only after confirming, with the first method", async () => {
    prPending();
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [open({ merge_methods: ["squash", "merge"] })] }));
    const merge = vi.spyOn(api, "mergePr").mockResolvedValue(pr);
    const { findByText, getByRole } = render(<PrView owner="PROJ-1" />);
    fireEvent.click(await findByText("Merge"));
    expect(merge).not.toHaveBeenCalled();
    const dialog = getByRole("dialog");
    expect(dialog.textContent).toContain("Merge #42");
    expect(dialog.textContent).toContain("main");
    expect(dialog.textContent).toContain("squash");
    fireEvent.click(within(dialog).getByText("Merge"));
    await waitFor(() => expect(merge).toHaveBeenCalledWith("PROJ-1", 7, "squash"));
  });

  it("disables Merge with a reason while checks fail", async () => {
    prPending();
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [open({ ci: "failure", merge_methods: ["merge"] })] }));
    const { findByText, getAllByText } = render(<PrView owner="PROJ-1" />);
    const btn = (await findByText("Merge")) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(getAllByText("Some checks are failing").length).toBeGreaterThan(0);
  });

  it("disables Merge when the allowed methods are unknown", async () => {
    prPending();
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [open({ merge_methods: [] })] }));
    const { findByText, getByText } = render(<PrView owner="PROJ-1" />);
    expect(((await findByText("Merge")) as HTMLButtonElement).disabled).toBe(true);
    expect(getByText("Couldn't tell which merge methods the repository allows")).toBeTruthy();
  });

  it("shows a 409 once and refreshes the PR", async () => {
    prPending();
    const taskPrs = vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [open({ merge_methods: ["merge"] })] }));
    vi.spyOn(api, "mergePr").mockRejectedValue(new ApiError("Checks are failing", 409));
    const { findByText, getByRole, findAllByText } = render(<PrView owner="PROJ-1" />);
    fireEvent.click(await findByText("Merge"));
    fireEvent.click(within(getByRole("dialog")).getByText("Merge"));
    expect((await findAllByText("Checks are failing")).length).toBe(1);
    await waitFor(() => expect(taskPrs.mock.calls.length).toBeGreaterThan(1));
  });

  describe("autofix", () => {
    const task = { key: "PROJ-1", status: { type: "running" }, ci_autofix: true } as unknown as Task;

    it("toggles the task's autofix from the header and tells the screen", async () => {
      vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
      const set = vi.spyOn(api, "setAutofix").mockResolvedValue({ ...task, ci_autofix: false });
      const changed = vi.fn();
      const { findByRole } = render(<PrView owner="PROJ-1" task={task} onTaskChanged={changed} />);
      const toggle = await findByRole("button", { name: /Fix CI/ });
      expect(toggle.getAttribute("aria-pressed")).toBe("true");
      fireEvent.click(toggle);
      await waitFor(() => expect(set).toHaveBeenCalledWith("PROJ-1", "off"));
      await waitFor(() => expect(toggle.getAttribute("aria-pressed")).toBe("false"));
      expect(changed).toHaveBeenCalled();
    });

    it("is disabled on a done task", async () => {
      vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
      const { findByRole } = render(<PrView owner="PROJ-1" task={{ ...task, status: { type: "done" } }} />);
      expect(((await findByRole("button", { name: /Fix CI/ })) as HTMLButtonElement).disabled).toBe(true);
    });

    it("is disabled, with no retry, on a failed task (the daemon answers 409 terminal)", async () => {
      const stopped = { state: "stopped" as const, attempts: 3, limit: 3, sha: "abc", sent_at: null, reason: "limit" as const };
      vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [{ ...pr, autofix: stopped }] }));
      const failed = { ...task, status: { type: "failed", reason: "x" } } as unknown as Task;
      const { findByRole, findByText } = render(<PrView owner="PROJ-1" task={failed} />);
      expect(((await findByRole("button", { name: /Fix CI/ })) as HTMLButtonElement).disabled).toBe(true);
      expect(((await findByText("Retry autofix")).closest("button") as HTMLButtonElement).disabled).toBe(true);
    });

    it("a 409 terminal reads translated and reloads the PRs", async () => {
      const taskPrs = vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
      vi.spyOn(api, "setAutofix").mockRejectedValue(new ApiError("task is finished", 409, false, "terminal"));
      const { findByRole, findByText } = render(<PrView owner="PROJ-1" task={task} />);
      fireEvent.click(await findByRole("button", { name: /Fix CI/ }));
      await findByText(/finished/i);
      expect(document.body.textContent).not.toContain("task is finished");
      await waitFor(() => expect(taskPrs.mock.calls.length).toBeGreaterThan(1));
    });

    it("reads off for every PR, with no retry, when the task's autofix is off", async () => {
      const stopped = { state: "stopped" as const, attempts: 3, limit: 3, sha: "abc", sent_at: null, reason: "limit" as const };
      vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [{ ...pr, autofix: stopped }] }));
      const { findByText, queryByText } = render(<PrView owner="PROJ-1" task={{ ...task, ci_autofix: false }} />);
      await findByText("Autofix: off");
      expect(queryByText("Retry autofix")).toBeNull();
    });

    it("a stopped PR says why and retries with its repo", async () => {
      const stopped = { state: "stopped" as const, attempts: 3, limit: 3, sha: "abc", sent_at: null, reason: "limit" as const };
      vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [{ ...pr, autofix: stopped }] }));
      const set = vi.spyOn(api, "setAutofix").mockResolvedValue(task);
      const { findByText } = render(<PrView owner="PROJ-1" task={task} />);
      await findByText(/Autofix: stopped · reached the attempt limit/);
      fireEvent.click(await findByText("Retry autofix"));
      await waitFor(() => expect(set).toHaveBeenCalledWith("PROJ-1", "retry", "api"));
    });

    it("a fixing PR shows the attempt, the failed checks and when it was sent", async () => {
      const fixing = { state: "fixing" as const, attempts: 1, limit: 3, sha: "abc", sent_at: new Date().toISOString(), reason: null };
      vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [{ ...pr, autofix: fixing }] }));
      const { findByText, container } = render(<PrView owner="PROJ-1" task={task} />);
      await findByText(/Fixing CI: attempt 1 of 3/);
      const line = container.querySelector(".pr-autofix")!.textContent!;
      expect(line).toContain("Failed checks: test");
      expect(line).toContain("sent");
    });

    it("a PR with no autofix record reads watching when the task has it on, off otherwise", async () => {
      vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
      const on = render(<PrView owner="PROJ-1" task={task} />);
      await on.findByText("Autofix: watching");
      cleanup();
      const off = render(<PrView owner="PROJ-1" task={{ ...task, ci_autofix: false }} />);
      await off.findByText("Autofix: off");
    });
  });
});
