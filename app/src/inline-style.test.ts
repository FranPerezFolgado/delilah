// Ratchet: inline `style={{` may only go down. Each file's count is capped by its entry;
// a file not listed may have none. Lower a number when you remove inline styles.
import { describe, expect, it } from "vitest";

const BASELINE: Record<string, number> = {
  "App.tsx": 2,
  "components/BacklogTaskView.tsx": 5,
  "components/BoardView.tsx": 4,
  "components/ChangesView.tsx": 8,
  "components/DeleteTaskDialog.tsx": 3,
  "components/FileTree.tsx": 4,
  "components/InboxView.tsx": 6,
  "components/JiraView.tsx": 17,
  "components/LogsView.tsx": 5,
  "components/NewTaskDialog.tsx": 16,
  "components/OpenPrDialog.tsx": 3,
  "components/PrView.tsx": 3,
  "components/ProjectSettings.tsx": 3,
  "components/ProjectView.tsx": 8,
  "components/ProjectsView.tsx": 21,
  "components/QuickOpen.tsx": 2,
  "components/SettingsView.tsx": 26,
  "components/SetupBanner.tsx": 3,
  "components/Sidebar.tsx": 10,
  "components/SpecPanel.tsx": 12,
  "components/StatusBar.tsx": 7,
  "components/TaskScreen.tsx": 14,
  "components/TaskView.tsx": 15,
  "components/UpdateNotice.tsx": 1,
};

const files = import.meta.glob<string>("./**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
});

describe("inline style ratchet", () => {
  it("no file exceeds its baseline", () => {
    const over: string[] = [];
    for (const [file, src] of Object.entries(files)) {
      if (/\.test\.tsx?$/.test(file)) continue;
      const rel = file.replace(/^\.\//, "");
      const n = (src.match(/style=\{\{/g) ?? []).length;
      const max = BASELINE[rel] ?? 0;
      if (n > max) over.push(`${rel}: ${n} > ${max}`);
    }
    expect(over).toEqual([]);
  });
});
