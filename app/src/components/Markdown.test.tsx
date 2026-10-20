import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DocContext } from "../markdown";
import { Markdown } from "./Markdown";

const doc: DocContext = {
  path: "docs/guide.md",
  version: "1",
  openFile: () => {},
  loadImage: () => Promise.resolve("data:image/png;base64,"),
};
const html = (text: string, props: Partial<Parameters<typeof Markdown>[0]> = {}) =>
  renderToStaticMarkup(<Markdown text={text} {...props} />);

describe("Markdown", () => {
  it("shows HTML in the text as text", () => {
    const out = html('<script>alert(1)</script> <img src=x onerror="alert(1)">\n\n<div>bloque</div>');
    expect(out).not.toMatch(/<script|<img|<div>bloque/);
    expect(out).toContain("&lt;script&gt;");
  });

  it("renders unsafe links as plain text", () => {
    const out = html("[js](javascript:alert(1)) [file](file:///etc/hosts) [up](../../../etc)", { doc });
    expect(out).not.toContain("<a");
    expect(out).toContain("<span>js</span>");
  });

  it("follows relative links only in workspace files", () => {
    expect(html("[otro](other.md)", { doc })).toContain('<a href="other.md"');
    expect(html("[otro](other.md)")).toContain("<span>otro</span>");
  });

  it("keeps single line breaks except in workspace files", () => {
    expect(html("uno\ndos")).toContain("<br/>");
    expect(html("uno\ndos", { doc })).not.toContain("<br/>");
  });

  it("gives headings prefixed ids, numbered when a title repeats", () => {
    const out = html("# Uso\n\n## Uso\n\n# Root");
    expect(out).toContain('id="md-uso"');
    expect(out).toContain('id="md-uso-1"');
    expect(out).toContain('id="md-root"');
    expect(out).not.toContain('id="root"');
  });

  it("renders GFM: tables, task lists and strikethrough", () => {
    const out = html("| a | b |\n|---|--:|\n| 1 | 2 |\n\n- [x] hecho\n- [ ] no\n\n~~fuera~~");
    expect(out).toContain('<div class="md-table"><table>');
    expect(out).toContain('style="text-align:right"');
    expect(out).toMatch(/<input type="checkbox" disabled="" readOnly="" class="md-task" checked=""\/>/);
    expect(out).toContain("<del>fuera</del>");
  });

  it("loads remote images only over https, and on a click for outside text", () => {
    const remote = "![logo](https://example.com/l.png) ![plain](http://example.com/l.png)";
    const shown = html(remote);
    expect(shown).toContain('<img src="https://example.com/l.png"');
    expect(shown).not.toContain('src="http://');
    const asked = html(remote, { remoteImages: "click" });
    expect(asked).not.toContain("<img");
    expect(asked).toContain("md-img-ask");
  });
});
