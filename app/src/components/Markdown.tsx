import { useMemo, useRef } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { HEADING_PREFIX, headingIds, resolveRelative, safeUrl, slug, type DocContext } from "../markdown";
import { openExternal } from "../platform";
import { MarkdownImage } from "./MarkdownImage";

interface Props {
  text: string;
  /** For a file of the workspace: its relative links and images point at other files. */
  doc?: DocContext;
  /** "click" for text from outside (tickets, agent messages): a remote image only loads
   *  when asked, so showing the text tells nobody it was read. */
  remoteImages?: "auto" | "click";
}

/** GitHub-flavoured Markdown for tickets, specs, agent messages and workspace files:
 *  tables, task lists, quotes, links, images… Builds React elements, never HTML (HTML in
 *  the text shows as written), and only follows the links `safeUrl` lets through: no
 *  link ever navigates the window. A file follows CommonMark; anything else keeps its
 *  single line breaks, as people write tickets and messages. */
export function Markdown({ text, doc, remoteImages = "auto" }: Props) {
  const root = useRef<HTMLDivElement>(null);
  // Callers pass a new `doc` each render: read it through a ref, so the components
  // below keep their identity and images are not fetched again.
  const docRef = useRef(doc);
  docRef.current = doc;
  const path = doc?.path;
  const version = doc?.version;

  const components = useMemo<Components>(() => {
    const scrollTo = (hash: string) => {
      let id: string;
      try {
        id = HEADING_PREFIX + slug(decodeURIComponent(hash));
      } catch {
        return;
      }
      root.current?.querySelector(`[id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "start" });
    };
    // th/td are not overridden: remark-gfm's column alignment already arrives as `style`.
    return {
      a: ({ href = "", children }) => {
        const kind = safeUrl(href);
        let onClick: () => void;
        if (kind === "external")
          onClick = () => void openExternal(href).catch((e) => console.error(`could not open ${href}`, e));
        else if (kind === "anchor") onClick = () => scrollTo(href.slice(1));
        else if (kind === "relative" && path !== undefined) {
          const resolved = resolveRelative(path, href);
          if (resolved === null) return <span>{children}</span>;
          onClick = () => docRef.current?.openFile(resolved);
        } else return <span>{children}</span>;
        return (
          <a
            href={href}
            title={href}
            onClick={(e) => {
              e.preventDefault();
              onClick();
            }}
            onAuxClick={(e) => e.preventDefault()}
          >
            {children}
          </a>
        );
      },
      img: ({ src, alt, title }) => {
        const target = typeof src === "string" ? src : "";
        const local = path !== undefined && safeUrl(target) === "relative" ? resolveRelative(path, target) : null;
        return (
          <MarkdownImage
            src={target}
            alt={alt}
            title={title}
            local={local}
            version={version}
            load={(p) => docRef.current?.loadImage(p) ?? Promise.reject(new Error("no document"))}
            askFirst={remoteImages === "click"}
          />
        );
      },
      input: ({ type, checked }) =>
        type === "checkbox" ? <input type="checkbox" checked={!!checked} disabled readOnly className="md-task" /> : null,
      table: ({ children }) => (
        <div className="md-table">
          <table>{children}</table>
        </div>
      ),
    };
  }, [path, version, remoteImages]);

  // The same element while the text is the same: redraws of the screen don't parse it again.
  const isFile = path !== undefined;
  const body = useMemo(
    () => (
      <ReactMarkdown
        remarkPlugins={isFile ? [remarkGfm, headingIds] : [remarkGfm, remarkBreaks, headingIds]}
        urlTransform={(url) => url}
        components={components}
      >
        {text}
      </ReactMarkdown>
    ),
    [text, components, isFile],
  );
  return (
    <div className="md selectable" ref={root}>
      {body}
    </div>
  );
}
