// What Markdown documents can point at, decided in one place: a link or an image is
// only followed when its target is one of these kinds.

export type UrlKind = "external" | "anchor" | "relative" | "blocked";

/** Whether a workspace file is Markdown, and so can be previewed. */
export const isMarkdown = (path: string) => /\.(md|markdown)$/i.test(path);

/** What a link or image target is. Unknown schemes (`javascript:`, `data:`, `file:`…)
 *  and protocol-relative `//host` are blocked. */
export function safeUrl(url: string): UrlKind {
  // Browsers ignore control characters and spaces in a scheme: so must we.
  const clean = url.replace(/[\u0000- \u007f]/g, "");
  if (clean === "") return "blocked";
  if (clean.startsWith("#")) return "anchor";
  if (clean.startsWith("//") || clean.startsWith("\\")) return "blocked";
  const scheme = /^([a-z][a-z0-9+.-]*):/i.exec(clean)?.[1].toLowerCase();
  if (scheme === undefined) return "relative";
  return scheme === "https" || scheme === "http" || scheme === "mailto" ? "external" : "blocked";
}

/** A relative target resolved against the folder of the document that has it, as a
 *  workspace path. Null when it leaves the workspace or is absolute. */
export function resolveRelative(docPath: string, target: string): string | null {
  const bare = target.split(/[?#]/)[0];
  if (bare === "" || bare.startsWith("/")) return null;
  let decoded: string;
  try {
    decoded = decodeURIComponent(bare);
  } catch {
    return null;
  }
  const parts = docPath.split("/").slice(0, -1);
  for (const part of decoded.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (parts.length === 0) return null;
      parts.pop();
    } else parts.push(part);
  }
  return parts.length ? parts.join("/") : null;
}

/** A heading's anchor, as GitHub makes them: `## Hola, mundo` → `hola-mundo`. */
export const slug = (text: string) =>
  text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .replace(/\s/g, "-");

/** What a document from the workspace offers its relative links and images. The
 *  component knows nothing about where files come from: whoever shows it does. */
export interface DocContext {
  /** The document's path in the workspace, the base for relative targets. */
  path: string;
  /** Changes when the document does, so its images load again. */
  version: string;
  openFile: (path: string) => void;
  /** A URL an `<img>` can show for a workspace image. */
  loadImage: (path: string) => Promise<string>;
}

/** Heading ids are prefixed, so a heading can't take the id of something in the app. */
export const HEADING_PREFIX = "md-";

interface MdNode {
  type: string;
  value?: string;
  children?: MdNode[];
  data?: { hProperties?: Record<string, unknown> };
}

const textOf = (node: MdNode): string => node.value ?? node.children?.map(textOf).join("") ?? "";

/** A remark plugin giving every heading an anchor id, numbered as GitHub does when the
 *  same title comes back: `md-uso`, `md-uso-1`… */
export function headingIds() {
  return (tree: MdNode) => {
    const seen = new Map<string, number>();
    const walk = (node: MdNode) => {
      if (node.type === "heading") {
        const base = slug(textOf(node));
        const n = seen.get(base) ?? 0;
        seen.set(base, n + 1);
        const id = HEADING_PREFIX + (n ? `${base}-${n}` : base);
        node.data = { ...node.data, hProperties: { ...node.data?.hProperties, id } };
      }
      node.children?.forEach(walk);
    };
    walk(tree);
  };
}
