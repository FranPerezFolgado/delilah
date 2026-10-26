// Final allow-list of inline `style={{`: each file's count must equal its entry;
// a file not listed may have none.
import { describe, expect, it } from "vitest";

// Only values that are computed at run time may be inline; everything else is a class.
const ALLOWED: Record<string, number> = {
  "components/TaskScreen.tsx": 1, // --tree-w, the dragged width of the project tree
  "playground/Floor.tsx": 3, // art-px × scale positions and sizes (label, floor), per-kind colour var in the legend
  "playground/FloorMini.tsx": 1, // art-px × scale size of the miniature's scene
  "playground/FloorScene.tsx": 2, // art-px × scale positions and sizes (bar, halo) of the desk art
  "playground/Timeline.tsx": 1, // per-kind colour var
};

const files = import.meta.glob<string>("./**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
});

describe("inline style allow-list", () => {
  it("no file exceeds its baseline", () => {
    const over: string[] = [];
    for (const [file, src] of Object.entries(files)) {
      if (/\.test\.tsx?$/.test(file)) continue;
      const rel = file.replace(/^\.\//, "");
      const n = (src.match(/style=\{\{/g) ?? []).length;
      const max = ALLOWED[rel] ?? 0;
      if (n !== max) over.push(`${rel}: ${n} instead of ${max}`);
    }
    expect(over).toEqual([]);
  });
});
