// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DiffRows } from "./DiffRows";

describe("DiffRows", () => {
  it("draws each row with its kind, line numbers and sign", () => {
    const { container } = render(
      <div>
        <DiffRows
          rows={[
            { kind: "hunk", old: null, new: null, text: "@@ -1,2 +1,2 @@" },
            { kind: "ctx", old: 1, new: 1, text: "a" },
            { kind: "del", old: 2, new: null, text: "b" },
            { kind: "add", old: null, new: 2, text: "c" },
            { kind: "note", old: 9, new: 9, text: "… cut" },
          ]}
        />
      </div>,
    );
    const rows = [...container.querySelectorAll(".dl")].map((r) => ({
      cls: r.className,
      lns: [...r.querySelectorAll(".ln")].map((l) => l.textContent),
      code: r.querySelector(".code")!.textContent,
    }));
    expect(rows).toEqual([
      { cls: "dl hunk", lns: ["", ""], code: "@@ -1,2 +1,2 @@" },
      { cls: "dl ctx", lns: ["1", "1"], code: " a" },
      { cls: "dl del", lns: ["2", ""], code: "−b" },
      { cls: "dl add", lns: ["", "2"], code: "+c" },
      { cls: "dl note", lns: ["", ""], code: "… cut" },
    ]);
  });
});
