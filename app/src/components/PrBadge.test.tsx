// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import type { PrSummary } from "../api";
import { PrBadge } from "./PrBadge";

afterEach(cleanup);

const base: PrSummary = { task: "PROJ-1", state: "open", draft: false, ci: "success", review_decision: "approved", count: 1 };

describe("PrBadge", () => {
  it("renders each state with its class", () => {
    for (const [pr, cls] of [
      [{ ...base, draft: true }, ".pr-draft"],
      [base, ".pr-open"],
      [{ ...base, state: "merged" }, ".pr-merged"],
      [{ ...base, state: "closed" }, ".pr-closed"],
    ] as [PrSummary, string][]) {
      const { container } = render(<PrBadge pr={pr} />);
      expect(container.querySelector(cls)).not.toBeNull();
      cleanup();
    }
  });

  it("shows the CI dot and the review in the tooltip", () => {
    const { container } = render(<PrBadge pr={{ ...base, ci: "failure", review_decision: "changes-requested" }} />);
    expect(container.querySelector(".pr-dot")?.getAttribute("data-ci")).toBe("failure");
    expect(container.querySelector(".pr-badge")?.getAttribute("title")).toContain("Changes requested");
  });

  it("shows the worst of a changeset and its count", () => {
    const { container } = render(<PrBadge pr={{ ...base, ci: "failure", count: 3 }} />);
    expect(container.querySelector(".pr-dot")?.getAttribute("data-ci")).toBe("failure");
    expect(container.querySelector(".pr-count")?.textContent).toBe("3");
  });

  it("has no dot without CI", () => {
    const { container } = render(<PrBadge pr={{ ...base, ci: "none" }} />);
    expect(container.querySelector(".pr-dot")).toBeNull();
  });
});
