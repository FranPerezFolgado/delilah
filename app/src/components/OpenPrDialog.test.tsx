// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, ApiError, type PrProposal, type PullRequest } from "../api";
import { OpenPrDialog } from "./OpenPrDialog";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const proposal: PrProposal = { repo: ".", title: "Add retries", body: "Body text", base: "main", needs_push: true, repos: ["."] };
const pr = { id: 1, number: 5 } as PullRequest;

describe("OpenPrDialog", () => {
  it("prefills the proposal and shows base and push note", async () => {
    vi.spyOn(api, "prProposal").mockResolvedValue(proposal);
    const { findByDisplayValue, container } = render(<OpenPrDialog owner="K-1" onClose={() => {}} onOpened={() => {}} />);
    await findByDisplayValue("Add retries");
    await findByDisplayValue("Body text");
    expect(container.textContent).toContain("against main");
    expect(container.textContent).toContain("The branch will be pushed before opening it");
  });

  it("sends the edited values as draft and reports the PR", async () => {
    vi.spyOn(api, "prProposal").mockResolvedValue(proposal);
    const open = vi.spyOn(api, "openPr").mockResolvedValue(pr);
    const onOpened = vi.fn();
    const { findByDisplayValue, getByText, getByRole } = render(<OpenPrDialog owner="K-1" onClose={() => {}} onOpened={onOpened} />);
    const title = await findByDisplayValue("Add retries");
    fireEvent.change(title, { target: { value: "Better title" } });
    fireEvent.click(getByRole("checkbox"));
    fireEvent.click(getByText("Open PR", { selector: "button" }));
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith("K-1", { repo: ".", title: "Better title", body: "Body text", draft: true }),
    );
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith(pr));
  });

  it("shows a failure inline and stays open", async () => {
    vi.spyOn(api, "prProposal").mockResolvedValue(proposal);
    vi.spyOn(api, "openPr").mockRejectedValue(new ApiError("remote: permission denied", 409));
    const onOpened = vi.fn();
    const { findByDisplayValue, getByText, findByText } = render(<OpenPrDialog owner="K-1" onClose={() => {}} onOpened={onOpened} />);
    await findByDisplayValue("Add retries");
    fireEvent.click(getByText("Open PR", { selector: "button" }));
    await findByText("remote: permission denied");
    expect(onOpened).not.toHaveBeenCalled();
    expect((getByText("Open PR", { selector: "button" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("shows no submodule select for a plain repo", async () => {
    vi.spyOn(api, "prProposal").mockResolvedValue(proposal);
    const { findByDisplayValue, container } = render(<OpenPrDialog owner="K-1" onClose={() => {}} onOpened={() => {}} />);
    await findByDisplayValue("Add retries");
    expect(container.querySelector("select")).toBeNull();
  });

  it("offers the submodules, reloads on change and sends the chosen repo", async () => {
    const repos = ["svc-a", "svc-b"];
    const prop = vi.spyOn(api, "prProposal").mockImplementation(async (_k, repo = ".") => ({
      ...proposal, repo: repo === "." ? "." : repo, title: `Title ${repo === "." ? "svc-a" : repo}`, repos,
    }));
    const open = vi.spyOn(api, "openPr").mockResolvedValue(pr);
    const { findByDisplayValue, container, getByText } = render(<OpenPrDialog owner="K-1" onClose={() => {}} onOpened={() => {}} />);
    await findByDisplayValue("Title svc-a");
    const select = container.querySelector("select")!;
    expect(Array.from(select.options).map((o) => o.value)).toEqual(repos);
    fireEvent.change(select, { target: { value: "svc-b" } });
    await findByDisplayValue("Title svc-b");
    expect(prop).toHaveBeenLastCalledWith("K-1", "svc-b");
    fireEvent.click(getByText("Open PR", { selector: "button" }));
    await waitFor(() => expect(open).toHaveBeenCalledWith("K-1", expect.objectContaining({ repo: "svc-b" })));
  });
});
