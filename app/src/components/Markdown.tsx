import type { ReactNode } from "react";

/** Just enough Markdown for ticket descriptions: headings, lists, code, paragraphs.
 *  Builds React elements, never HTML, so ticket text cannot inject markup. */
export function Markdown({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  const lines = text.split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.startsWith("```")) {
      const code: string[] = [];
      for (i++; i < lines.length && !lines[i].startsWith("```"); i++) code.push(lines[i]);
      i++;
      blocks.push(<pre key={blocks.length} className="md-code">{code.join("\n")}</pre>);
    } else if (/^#{1,6} /.test(line)) {
      blocks.push(<div key={blocks.length} className="md-h">{inline(line.replace(/^#+ /, ""))}</div>);
      i++;
    } else if (/^\s*([-*]|\d+\.) /.test(line)) {
      const items: ReactNode[] = [];
      for (; i < lines.length && /^\s*([-*]|\d+\.) /.test(lines[i]); i++) {
        const depth = Math.floor((lines[i].match(/^\s*/)?.[0].length ?? 0) / 2);
        items.push(
          <li key={i} style={{ marginLeft: depth * 16 }}>
            {inline(lines[i].replace(/^\s*([-*]|\d+\.) /, ""))}
          </li>,
        );
      }
      blocks.push(<ul key={blocks.length} className="md-list">{items}</ul>);
    } else if (line.trim() === "") {
      i++;
    } else {
      const para: string[] = [];
      for (; i < lines.length && lines[i].trim() !== "" && !/^(#{1,6} |```|\s*([-*]|\d+\.) )/.test(lines[i]); i++) para.push(lines[i]);
      blocks.push(<p key={blocks.length} className="md-p">{inline(para.join("\n"))}</p>);
    }
  }
  return <div className="md selectable">{blocks}</div>;
}

function inline(text: string): ReactNode[] {
  return text.split(/(`[^`]+`)/g).map((part, i) =>
    part.startsWith("`") && part.endsWith("`") && part.length > 1 ? <code key={i}>{part.slice(1, -1)}</code> : part,
  );
}
