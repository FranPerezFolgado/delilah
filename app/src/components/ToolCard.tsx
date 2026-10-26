import { useState } from "react";
import { api, type ToolEntry } from "../api";
import { t, tn } from "../i18n";
import { DiffRows } from "./DiffRows";
import "./ToolCard.css";

interface Props {
  entry: ToolEntry;
  sessionId: string;
  onOpenFile: (path: string) => void;
}

const str = (v: unknown): string => (typeof v === "string" ? v : "");

/** A tool call of the agent: one summary line, expandable to what it did. Everything
 *  comes from the repo or the agent, so it is shown as text, never as HTML. */
export function ToolCard({ entry, sessionId, onOpenFile }: Props) {
  const [open, setOpen] = useState(false);
  const [full, setFull] = useState<{ input: Record<string, unknown>; result: string | null } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // A truncated or malformed tool_use line may come without an input object.
  const raw: unknown = full?.input ?? entry.input;
  const input: Record<string, unknown> = raw && typeof raw === "object" && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
  const result = full ? full.result : entry.result;
  const pending = entry.result === null;
  const truncated = entry.truncated && !full;
  const path = entry.path ?? str(input.file_path);

  const showAll = () => {
    setLoading(true);
    setError(null);
    api.toolDetail(sessionId, entry.tool_use_id).then(
      (d) => {
        setFull({ input: d.input, result: d.result });
        setLoading(false);
      },
      (e) => {
        setError((e as Error).message);
        setLoading(false);
      },
    );
  };

  const output = result ? <pre className="tool-out selectable">{result}</pre> : null;
  const count = result ? result.split("\n").filter((l) => l.trim()).length : 0;

  const body = () => {
    switch (entry.tool) {
      case "Bash":
        return (
          <>
            <pre className="tool-cmd mono selectable">{str(input.command)}</pre>
            {output}
          </>
        );
      case "Edit":
      case "MultiEdit":
      case "Write":
      case "NotebookEdit":
        return (
          <>
            <div className="tool-path">
              <span className="mono selectable trunc">{path}</span>
              {path && !path.startsWith("/") && (
                <button className="btn btn-ghost" onClick={() => onOpenFile(path)}>
                  {t("Abrir en el editor")}
                </button>
              )}
            </div>
            {entry.diff && (
              <div className="diff selectable">
                <DiffRows rows={entry.diff} />
              </div>
            )}
          </>
        );
      case "Read": {
        const offset = typeof input.offset === "number" ? input.offset : null;
        const limit = typeof input.limit === "number" ? input.limit : null;
        // `offset` is the first line read, `limit` how many: 10 and 5 are lines 10-14.
        const range = offset === null && limit === null ? null : `${offset ?? 1}-${limit === null ? "" : (offset ?? 1) + limit - 1}`;
        return (
          <div className="tool-path">
            <span className="mono selectable trunc">{path}</span>
            {range && <span className="muted mono">{range}</span>}
          </div>
        );
      }
      case "Grep":
      case "Glob":
        return (
          <>
            <div className="tool-path">
              <span className="mono selectable">{str(input.pattern)}</span>
              {!pending && <span className="muted">{tn(count, "{n} resultado", "{n} resultados")}</span>}
            </div>
            {output}
          </>
        );
      case "TodoWrite": {
        const todos = Array.isArray(input.todos) ? (input.todos as Record<string, unknown>[]) : [];
        return (
          <ul className="tool-todos">
            {todos.map((td, i) => (
              <li key={i} className={str(td.status) === "completed" ? "done" : ""}>
                {str(td.content)}
              </li>
            ))}
          </ul>
        );
      }
      case "Task":
      case "Agent":
        return (
          <>
            <div className="muted">{t("Encargo")}</div>
            <pre className="tool-out selectable">{str(input.prompt)}</pre>
            {result && (
              <>
                <div className="muted">{t("Resultado")}</div>
                {output}
              </>
            )}
          </>
        );
      default:
        return (
          <>
            <div className="mono">{entry.tool}</div>
            <pre className="tool-out selectable">{JSON.stringify(input, null, 2)}</pre>
            {output}
          </>
        );
    }
  };

  const state = pending ? "running" : entry.is_error ? "failed" : "";
  return (
    <div className={`tool-card ${state}`.trim()} role="group" aria-label={entry.summary}>
      <button className="tool-head" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span className="muted">{open ? "▾" : "▸"}</span>
        <span className="tool-sum mono trunc">{entry.summary}</span>
      </button>
      {open && (
        <div className="tool-body">
          {body()}
          {pending && <div className="muted">{t("En curso…")}</div>}
          {entry.is_error && <div className="error">{t("Falló")}</div>}
          {truncated && (
            <button className="btn btn-ghost" disabled={loading} onClick={showAll}>
              {t("Ver todo")}
            </button>
          )}
          {error && <div className="error">{error}</div>}
        </div>
      )}
    </div>
  );
}
