import { useEffect, useState } from "react";
import { api, type SpecProgress, type Task } from "../api";
import { COLUMNS, columnOf, type Column } from "../phases";
import { Icon } from "./Icon";
import { Markdown } from "./Markdown";

/** A task's SpecKit spec: the phase it is in, its documents and the tasks.md checklist. */
export function SpecPanel({ task }: { task: Task }) {
  const [spec, setSpec] = useState<SpecProgress | null>(null);
  const [open, setOpen] = useState<{ name: string; text: string } | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      api
        .spec(task.key)
        .then((s) => alive && setSpec(s))
        .catch(() => {});
    load();
    const timer = setInterval(load, 3000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [task.key]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(null);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  // No workspace (yet, or archived): nothing to follow.
  if (!spec) return null;

  const column = columnOf(task, spec);
  const at = COLUMNS.findIndex((c) => c.id === column);
  const running = task.status.type === "running";
  const showFile = (name: string) =>
    api
      .specFile(task.key, name)
      .then((text) => setOpen({ name, text }))
      .catch(() => {});

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <Stepper at={at} />

      {!spec.dir ? (
        <div className="muted" style={{ fontSize: 12 }}>
          Aún no hay carpeta de spec en <span className="mono">specs/</span>. Aparece aquí en cuanto el agente la cree.
        </div>
      ) : (
        <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
          <span className="mono muted selectable" style={{ fontSize: 11.5, marginRight: 4 }}>
            {spec.dir}/
          </span>
          {spec.files.map((f) => (
            <button key={f} className="btn btn-ghost chip" onClick={() => showFile(f)}>
              {f}
            </button>
          ))}
        </div>
      )}

      {spec.total > 0 && (
        <div>
          <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 8 }}>
            <span className="sec">Subtareas</span>
            <span className="bar" style={{ width: 120 }}>
              <span style={{ width: `${(spec.done / spec.total) * 100}%`, background: spec.done === spec.total ? "var(--done)" : "var(--run)" }} />
            </span>
            <span className="mono muted" style={{ fontSize: 11.5 }}>
              {spec.done}/{spec.total}
            </span>
          </div>
          <div className="card checklist">
            {spec.groups.map((g, gi) => {
              const done = g.items.filter((i) => i.done).length;
              return (
                <details key={gi} open={done < g.items.length}>
                  <summary>
                    <span className="trunc" style={{ flex: 1 }}>{g.title || "Tareas"}</span>
                    <span className="mono muted">
                      {done}/{g.items.length}
                    </span>
                  </summary>
                  {g.items.map((item, ii) => {
                    const current = !item.done && `${item.id ? item.id + " " : ""}${item.text}` === spec.current;
                    return (
                      <div key={ii} className={`check ${item.done ? "is-done" : ""} ${current ? "is-current" : ""}`}>
                        <span className={`box ${item.done ? "c-done" : "c-mut"}`}>{item.done ? <Icon name="done" /> : <span className="empty-box" />}</span>
                        {item.id && <span className="mono muted">{item.id}</span>}
                        <span className="selectable" style={{ flex: 1 }}>{item.text}</span>
                        {item.parallel && <span className="tag" title="Se puede hacer en paralelo">P</span>}
                        {item.story && <span className="tag">{item.story}</span>}
                        {current && <span className="tag tag-run">{running ? "en curso" : "siguiente"}</span>}
                      </div>
                    );
                  })}
                </details>
              );
            })}
          </div>
        </div>
      )}

      {open && (
        <div className="backdrop" onClick={() => setOpen(null)}>
          <div className="dialog doc" onClick={(e) => e.stopPropagation()} role="dialog" aria-label={open.name}>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <h2 className="mono" style={{ flex: 1 }}>{open.name}</h2>
              <button className="btn btn-ghost" onClick={() => setOpen(null)} aria-label="Cerrar">
                <Icon name="close" />
              </button>
            </div>
            <Markdown text={open.text} />
          </div>
        </div>
      )}
    </div>
  );
}

function Stepper({ at }: { at: number }) {
  return (
    <ol className="stepper">
      {COLUMNS.map((c: { id: Column; label: string }, i) => (
        <li key={c.id} className={i < at ? "past" : i === at ? "now" : ""}>
          <span className="dot-step">{i < at ? <Icon name="done" /> : i + 1}</span>
          {c.label}
        </li>
      ))}
    </ol>
  );
}
