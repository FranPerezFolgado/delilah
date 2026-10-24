import { describe, expect, it } from "vitest";
import cases from "../../crates/delilah-core/tests/branch_names.json";
import { branchName } from "./branchName";

describe("branchName", () => {
  it.each(cases)("$template / $key / $title", ({ template, key, title, branch }) => {
    expect(branchName(template, key, title)).toBe(branch);
  });
});
