import { describe, expect, it } from "vitest";
import type { BackgroundInfo, ConversationEntry } from "./api";
import { foldBackground, outcome } from "./backgroundItems";

const b = (o: Partial<BackgroundInfo>): BackgroundInfo => ({ source: "notification", kind: "command", ...o });

describe("outcome", () => {
  it("maps statuses", () => {
    expect(outcome(b({ status: "failed" }))).toBe("failed");
    expect(outcome(b({ status: "completed", exit_code: 2 }))).toBe("failed");
    expect(outcome(b({ status: "stopped" }))).toBe("stopped");
    expect(outcome(b({ status: "killed" }))).toBe("stopped");
    expect(outcome(b({ status: "completed" }))).toBe("ok");
    expect(outcome(b({ status: "completed", exit_code: 0 }))).toBe("ok");
    expect(outcome(b({ exit_code: 0 }))).toBe("ok");
  });
  it("is other when missing or unknown", () => {
    expect(outcome(b({}))).toBe("other");
    expect(outcome(b({ status: "weird" }))).toBe("other");
  });
});

type Marker = Extract<ConversationEntry, { kind: "marker" }>;
const mk = (id: string, o: Partial<BackgroundInfo>): Marker => ({
  id, at: `2025-01-01T00:00:0${id.length}Z`, kind: "marker", marker: "background-done", background: b(o),
});
const note = (id: string, task: string, o: Partial<BackgroundInfo> = {}) =>
  mk(id, { source: "notification", kind: "agent", task_id: task, report_in_handback: true, tokens: 5, ...o });
const hand = (id: string, task: string, o: Partial<BackgroundInfo> = {}) =>
  mk(id, { source: "handback", kind: "agent", task_id: task, report: `R-${id}`, ...o });
const user = (id: string): ConversationEntry => ({ id, at: "2025-01-01T00:00:00Z", kind: "user", text: id, state: "sent", from: "view" });

describe("foldBackground", () => {
  it("returns an empty list for no entries", () => {
    expect(foldBackground([])).toEqual([]);
  });

  it("merges a notification with a later hand-back", () => {
    const out = foldBackground([note("n", "a"), user("u"), hand("h", "a", { report_truncated: true })]);
    expect(out.map((e) => e.id)).toEqual(["n", "u"]);
    const m = out[0] as Marker;
    expect(m.background).toMatchObject({ tokens: 5, report: "R-h", report_truncated: true, source: "notification" });
    expect(m.at).toBe(note("n", "a").at);
  });
  it("merges a notification with an earlier hand-back", () => {
    const out = foldBackground([hand("h", "a"), user("u"), note("n", "a")]);
    expect(out.map((e) => e.id)).toEqual(["u", "n"]);
    expect((out[1] as Marker).background?.report).toBe("R-h");
  });
  it("pairs repeated task ids one to one, nearest first", () => {
    const out = foldBackground([note("n1", "a"), hand("h1", "a"), user("u"), note("n2", "a"), hand("h2", "a")]);
    expect(out.map((e) => e.id)).toEqual(["n1", "u", "n2"]);
    expect((out[0] as Marker).background?.report).toBe("R-h1");
    expect((out[2] as Marker).background?.report).toBe("R-h2");
  });
  it("leaves unmatched hand-backs and placeholders alone", () => {
    const input = [hand("h", "x"), note("n", "a"), note("m", "b", { report_in_handback: false, report: "inline" })];
    expect(foldBackground(input)).toEqual(input);
  });
  it("does not mutate and is idempotent", () => {
    const input = [note("n", "a"), hand("h", "a")];
    const copy = structuredClone(input);
    const a = foldBackground(input);
    expect(input).toEqual(copy);
    expect(foldBackground(input)).toEqual(a);
  });
});
