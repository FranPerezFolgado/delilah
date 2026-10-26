import { expect, it } from "vitest";
import { levelOf, parseLogs, passes } from "./logLevel";

it("reads the tracing level token", () => {
  expect(levelOf("2026-10-26T10:00:00.123Z  INFO delilah_daemon: listening")).toBe("info");
  expect(levelOf("2026-10-26T10:00:00Z WARN x: slow")).toBe("warn");
  expect(levelOf("2026-10-26T10:00:00Z ERROR x: boom")).toBe("error");
  expect(levelOf("2026-10-26T10:00:00Z DEBUG x: y")).toBe("debug");
  expect(levelOf("2026-10-26T10:00:00Z TRACE x: y")).toBe("trace");
});
it("does not take a word inside the message", () => {
  expect(levelOf("plain line")).toBeNull();
  expect(levelOf("2026-10-26T10:00:00Z  INFO x: the INFORMATION and ERRORS")).toBe("info");
  expect(levelOf("")).toBeNull();
});
it("continuation lines inherit the previous level", () => {
  const p = parseLogs("orphan\nT ERROR a: failed\n  caused by x\nT INFO b: ok\n  more");
  expect(p.map((x) => x.level)).toEqual([null, "error", "error", "info", "info"]);
});
it("filters by minimum level", () => {
  expect(passes("debug", "all")).toBe(true);
  expect(passes(null, "all")).toBe(true);
  expect(passes("debug", "info")).toBe(false);
  expect(passes("info", "info")).toBe(true);
  expect(passes("warn", "info")).toBe(true);
  expect(passes("info", "warn")).toBe(false);
  expect(passes("error", "warn")).toBe(true);
  expect(passes("warn", "error")).toBe(false);
  expect(passes(null, "error")).toBe(false);
});
