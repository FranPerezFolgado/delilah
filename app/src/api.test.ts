import { describe, expect, it } from "vitest";
import { baseStaleNotices, type InboxItem } from "./api";

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
