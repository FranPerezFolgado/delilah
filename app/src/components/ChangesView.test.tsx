// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type FileChange, type RepoChanges } from "../api";
import { ChangesView } from "./ChangesView";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const file = (path: string, kind: FileChange["kind"] = "modified"): FileChange => ({
  path, old_path: null, kind, additions: 2, deletions: 1,
});
const repo = (files: FileChange[]): RepoChanges => ({ repo: ".", base: "abcdef123456", commits: 1, files });

describe("ChangesView", () => {
  it("shows a skeleton while loading and no empty text until loaded", async () => {
    let done!: (r: RepoChanges[]) => void;
    vi.spyOn(api, "changes").mockReturnValue(new Promise((res) => (done = res)));
    const { container, queryByText, findByText } = render(<ChangesView owner="K-1" onEdit={() => {}} />);
    expect(container.querySelector(".sk")).not.toBeNull();
    expect(queryByText("The branch has not changed anything yet.")).toBeNull();
    done([repo([])]);
    await findByText("The branch has not changed anything yet.");
    expect(container.querySelector(".sk")).toBeNull();
  });

  it("groups files by folder with A/M/D letters and counts through tn", async () => {
    vi.spyOn(api, "changes").mockResolvedValue([
      repo([file("README.md", "added"), file("src/a.ts"), file("src/b.ts", "deleted")]),
    ]);
    vi.spyOn(api, "diff").mockResolvedValue("");
    const { findByRole, container } = render(<ChangesView owner="K-1" onEdit={() => {}} />);
    const src = await findByRole("group", { name: "src" });
    expect([...src.querySelectorAll(".frow")].map((r) => r.textContent?.replace(/[+−]\d+/g, ""))).toEqual(["Ma.ts", "Db.ts"]);
    expect(container.querySelector(".changes-summary")?.textContent).toContain("3 files");
    expect(container.querySelector(".frow .kind")?.textContent).toBe("A");
  });

  it("filters by path and says when nothing matches", async () => {
    vi.spyOn(api, "changes").mockResolvedValue([repo([file("src/a.ts"), file("docs/b.md")])]);
    vi.spyOn(api, "diff").mockResolvedValue("");
    const { findByLabelText, container, findByText } = render(<ChangesView owner="K-1" onEdit={() => {}} />);
    const input = await findByLabelText("Filter files");
    fireEvent.change(input, { target: { value: "docs" } });
    expect(container.querySelectorAll(".frow").length).toBe(1);
    expect(container.querySelector(".changes-summary")?.textContent).toContain("1 file");
    fireEvent.change(input, { target: { value: "zzz" } });
    await findByText("No file matches the filter.");
  });

  it("reloads with the Reload button", async () => {
    const changes = vi.spyOn(api, "changes").mockResolvedValue([repo([])]);
    const { findByText } = render(<ChangesView owner="K-1" onEdit={() => {}} />);
    fireEvent.click(await findByText("Reload"));
    await waitFor(() => expect(changes).toHaveBeenCalledTimes(2));
  });
});
