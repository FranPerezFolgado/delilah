import "./DiffRows.css";
export type DiffRowKind = "add" | "del" | "ctx" | "hunk" | "note";

export interface DiffRowData {
  kind: DiffRowKind;
  old: number | null;
  new: number | null;
  text: string;
}

/** The rows of a diff with old and new line numbers (the `.diff` block's content). */
export function DiffRows({ rows }: { rows: DiffRowData[] }) {
  return (
    <>
      {rows.map((r, i) =>
        r.kind === "hunk" || r.kind === "note" ? (
          <div key={i} className={`dl ${r.kind}`}>
            <span className="ln" />
            <span className="ln" />
            <span className="code">{r.text}</span>
          </div>
        ) : (
          <div key={i} className={`dl ${r.kind}`}>
            <span className="ln">{r.old ?? ""}</span>
            <span className="ln">{r.new ?? ""}</span>
            <span className="code">
              <span className="sign">{r.kind === "add" ? "+" : r.kind === "del" ? "−" : " "}</span>
              {r.text}
            </span>
          </div>
        ),
      )}
    </>
  );
}
