import { beforeEach, describe, expect, it } from "vitest";

// session.ts keeps its state in browser storage: a Map stands in for it.
const store = new Map<string, string>();
globalThis.localStorage = {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, v),
  removeItem: (k: string) => void store.delete(k),
  clear: () => store.clear(),
  key: () => null,
  get length() {
    return store.size;
  },
} as Storage;

const { OVERVIEW, persist, sessionFor } = await import("./session");

const tab = (path: string, preview: boolean) => ({
  path,
  version: "1",
  size: 1,
  saved: null,
  dirty: false,
  conflict: false,
  preview,
});

describe("session persistence", () => {
  beforeEach(() => store.clear());

  it("brings back which Markdown tabs were shown rendered", () => {
    const s = sessionFor("A-1");
    s.tabs = [tab("README.md", true), tab("src/main.rs", false)];
    s.pending = ["docs/later.md"];
    s.pendingPreview = new Set(["docs/later.md", "gone.md"]);
    persist("A-1", s);

    const saved = JSON.parse(store.get("delilah.tabs.A-1")!);
    expect(saved.files).toEqual(["README.md", "src/main.rs", "docs/later.md"]);
    expect(saved.previews).toEqual(["README.md", "docs/later.md"]);

    const again = sessionFor("A-2");
    expect(again.pendingPreview.size).toBe(0);
    store.set("delilah.tabs.A-3", JSON.stringify(saved));
    expect([...sessionFor("A-3").pendingPreview]).toEqual(["README.md", "docs/later.md"]);
  });

  it("reads what older versions saved, without previews", () => {
    store.set("delilah.tabs.B-1", JSON.stringify({ active: OVERVIEW, files: ["a.md"], changesOpen: false, expanded: [] }));
    const s = sessionFor("B-1");
    expect(s.pending).toEqual(["a.md"]);
    expect(s.pendingPreview.size).toBe(0);
  });
});
