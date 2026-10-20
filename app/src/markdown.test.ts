import { describe, expect, it } from "vitest";
import { isMarkdown, resolveRelative, safeUrl, slug } from "./markdown";

describe("safeUrl", () => {
  it("lets web and mail links through", () => {
    for (const url of ["https://example.com", "http://example.com/a?b#c", "mailto:a@b.c", "HTTPS://X"]) {
      expect(safeUrl(url), url).toBe("external");
    }
  });

  it("blocks every other scheme, however it is disguised", () => {
    for (const url of [
      "javascript:alert(1)",
      " JaVaScRiPt:alert(1)",
      "java\tscript:alert(1)",
      "java\u0000script:alert(1)",
      "data:text/html,<b>",
      "file:///etc/passwd",
      "vbscript:x",
      "//evil.example",
      "\\\\evil",
      "",
    ]) {
      expect(safeUrl(url), JSON.stringify(url)).toBe("blocked");
    }
  });

  it("tells anchors from relative paths", () => {
    expect(safeUrl("#uso")).toBe("anchor");
    expect(safeUrl(" #uso")).toBe("anchor");
    expect(safeUrl("docs/a.md")).toBe("relative");
    expect(safeUrl("../a.png")).toBe("relative");
  });
});

describe("resolveRelative", () => {
  it("resolves against the document's folder", () => {
    expect(resolveRelative("docs/a.md", "b.md")).toBe("docs/b.md");
    expect(resolveRelative("docs/a.md", "./sub/c.md")).toBe("docs/sub/c.md");
    expect(resolveRelative("docs/nested/a.md", "../b.md")).toBe("docs/b.md");
    expect(resolveRelative("docs/a.md", "../README.md")).toBe("README.md");
  });

  it("drops the query and the hash and decodes the path", () => {
    expect(resolveRelative("docs/a.md", "b.md?v=1#uso")).toBe("docs/b.md");
    expect(resolveRelative("docs/a.md", "img%20one.png")).toBe("docs/img one.png");
  });

  it("never leaves the workspace", () => {
    expect(resolveRelative("docs/a.md", "../../x")).toBeNull();
    expect(resolveRelative("docs/a.md", "..%2F..%2Fetc%2Fpasswd")).toBeNull();
    expect(resolveRelative("a.md", "..")).toBeNull();
    expect(resolveRelative("a.md", "/etc/passwd")).toBeNull();
    expect(resolveRelative("a.md", "")).toBeNull();
    expect(resolveRelative("a.md", "%E0%A4%A")).toBeNull();
  });
});

describe("isMarkdown", () => {
  it("knows Markdown by its extension", () => {
    expect(isMarkdown("README.md")).toBe(true);
    expect(isMarkdown("docs/NOTES.MD")).toBe(true);
    expect(isMarkdown("a.markdown")).toBe(true);
    expect(isMarkdown("a.md.txt")).toBe(false);
    expect(isMarkdown("a.mdx")).toBe(false);
    expect(isMarkdown("README")).toBe(false);
  });
});

describe("slug", () => {
  it("makes anchors as GitHub does", () => {
    expect(slug("Hola, Mundo")).toBe("hola-mundo");
    expect(slug("  Paso 2: probar  ")).toBe("paso-2-probar");
    expect(slug("Añadir ñ")).toBe("añadir-ñ");
    expect(slug("a  b")).toBe("a--b");
  });
});
