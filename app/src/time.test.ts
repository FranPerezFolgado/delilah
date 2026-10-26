import { afterEach, describe, expect, it } from "vitest";
import { setLanguage } from "./i18n";
import { relativeTime } from "./time";

const NOW = new Date(2026, 9, 26, 15, 0, 0);
const ago = (ms: number) => new Date(NOW.getTime() - ms);
const MIN = 60_000;
const H = 60 * MIN;
const D = 24 * H;

afterEach(() => setLanguage("es"));

describe("relativeTime (es)", () => {
  it("covers moment, minutes, hours, yesterday, days and dates", () => {
    setLanguage("es");
    expect(relativeTime(ago(10_000), NOW)).toBe("hace un momento");
    expect(relativeTime(ago(4 * MIN), NOW)).toBe("hace 4 min");
    expect(relativeTime(ago(3 * H), NOW)).toBe("hace 3 h");
    expect(relativeTime(new Date(2026, 9, 25, 9, 0), NOW)).toBe("ayer");
    expect(relativeTime(new Date(2026, 9, 24, 23, 0), NOW)).toBe("hace 2 días");
    expect(relativeTime(ago(10 * D), NOW)).toBe(new Date(ago(10 * D)).toLocaleDateString("es-ES", { day: "numeric", month: "short" }));
  });
  it("accepts ISO strings and clamps the future to now", () => {
    setLanguage("es");
    expect(relativeTime(ago(2 * MIN).toISOString(), NOW)).toBe("hace 2 min");
    expect(relativeTime(new Date(NOW.getTime() + H), NOW)).toBe("hace un momento");
  });
});

describe("relativeTime (en)", () => {
  it("uses English", () => {
    setLanguage("en");
    expect(relativeTime(ago(10_000), NOW)).toBe("just now");
    expect(relativeTime(ago(4 * MIN), NOW)).toBe("4 min ago");
    expect(relativeTime(new Date(2026, 9, 25, 9, 0), NOW)).toBe("yesterday");
    expect(relativeTime(new Date(2026, 9, 24, 23, 0), NOW)).toBe("2 days ago");
  });
});
