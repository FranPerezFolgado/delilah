// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, ApiError, type PullRequest, type TaskPrs } from "../api";
import { PrView } from "./PrView";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const pr: PullRequest = {
  id: 7, repo: "api", host: "github.com", owner_name: "me/repo", number: 42, url: "https://github.com/me/repo/pull/42",
  linked_by: "detected", checked_at: new Date().toISOString(), error: null,
  snapshot: {
    state: "open", draft: false, title: "Reintentos", head: "abc", base: "main",
    checks: [{ name: "test", outcome: "failure", url: "https://ci/1" }],
    ci: "failure", review_decision: "changes-requested", comments: 3,
  },
};
const data = (over: Partial<TaskPrs>): TaskPrs => ({ account: "me", problem: null, prs: [pr], ...over });

describe("PrView", () => {
  it("shows the PR, its checks, review and repo", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({}));
    const { findByText, container } = render(<PrView owner="PROJ-1" />);
    await findByText("#42 Reintentos");
    expect(container.querySelector(".pr-chip")?.textContent).toBe("Open");
    expect(container.textContent).toContain("Changes requested");
    expect(container.textContent).toContain("into main");
    expect(container.textContent).toContain("test");
    expect(container.textContent).toContain("api");
    expect(container.textContent).toContain("me");
  });

  it("explains a problem with its fix", async () => {
    vi.spyOn(api, "taskPrs").mockResolvedValue(data({ prs: [], problem: { kind: "signed-out", message: "gh no tiene sesión" } }));
    const { findByText } = render(<PrView owner="PROJ-1" />);
    await findByText("Run gh auth login");
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
    await findByText("#42 Reintentos");
    fireEvent.click(getByText("Refresh"));
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
    vi.spyOn(api, "prProposal").mockResolvedValue({ repo: ".", title: "T", body: "", base: "main", needs_push: false });
    const { findByText, findByDisplayValue } = render(<PrView owner="PROJ-1" />);
    fireEvent.click(await findByText("Open PR"));
    await findByDisplayValue("T");
  });
});
