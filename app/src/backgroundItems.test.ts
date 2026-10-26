import { describe, expect, it } from "vitest";
import type { BackgroundInfo } from "./api";
import { outcome } from "./backgroundItems";

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
