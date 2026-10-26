import { useEffect, useState } from "react";
import { api, type SpecProgress, type Task } from "../api";
import { on } from "../live";
import { Button, Dialog, Icon } from "./ui";
import { Markdown } from "./Markdown";
import "./SpecPanel.css";
import { t } from "../i18n";

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
    const off = on("specs", load);
    return () => {
      alive = false;
      off();
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

  const running = task.status.type === "running";
  const showFile = (name: string) =>
    api
      .specFile(task.key, name)
      .then((text) => setOpen({ name, text }))
      .catch(() => {});
  const others = spec.files.filter((f) => f !== "tasks.md");

  return (
    <section className="sp-section">
      <div className="sp-title">
        <h3>{t("Tareas de la spec")}</h3>
        {spec.total > 0 && (
          <>
            <span className="sp-bar" role="progressbar" aria-valuemin={0} aria-valuemax={spec.total} aria-valuenow={spec.done}>
              <span className={spec.done === spec.total ? "sp-bar-ok" : "sp-bar-run"} ref={(el) => {
                if (el) el.style.width = `${(spec.done / spec.total) * 100}%`;
              }} />
            </span>
            <span className="mono sp-count">{spec.done}/{spec.total}</span>
          </>
        )}
        {spec.files.includes("tasks.md") && (
          <Button variant="ghost" size={28} onClick={() => showFile("tasks.md")}>
            {t("Abrir tasks.md")}
          </Button>
        )}
      </div>

      {!spec.dir ? (
        <div className="sp-empty">{t("Aún no hay carpeta de spec en specs/. Aparece aquí en cuanto el agente la cree.")}</div>
      ) : (
        <div className="sp-files">
          <span className="mono selectable sp-dir">{spec.dir}/</span>
          {others.map((f) => (
            <button key={f} type="button" className="sp-chip mono" onClick={() => showFile(f)}>
              {f}
            </button>
          ))}
        </div>
      )}

      {spec.total > 0 && (
        <div className="sp-card sp-checklist">
          {spec.groups.map((g, gi) => {
            const done = g.items.filter((i) => i.done).length;
            return (
              <details key={gi} open={done < g.items.length}>
                <summary>
                  <span className="sp-grow sp-trunc">{g.title || t("Tareas")}</span>
                  <span className="mono sp-count">
                    {done}/{g.items.length}
                  </span>
                </summary>
                {g.items.map((item, ii) => {
                  const now = !item.done && `${item.id ? item.id + " " : ""}${item.text}` === spec.current;
                  return (
                    <div key={ii} className={`sp-check ${item.done ? "is-done" : ""} ${now ? "is-current" : ""}`}>
                      <span className={item.done ? "c-done" : "c-mut"}>{item.done ? <Icon name="done" /> : <span className="sp-box" />}</span>
                      {item.id && <span className="mono sp-count">{item.id}</span>}
                      <span className="selectable sp-grow">{item.text}</span>
                      {item.parallel && <span className="sp-tag" title={t("Se puede hacer en paralelo")}>P</span>}
                      {item.story && <span className="sp-tag">{item.story}</span>}
                      {now && <span className="sp-tag sp-tag-run">{running ? t("en curso") : t("siguiente")}</span>}
                    </div>
                  );
                })}
              </details>
            );
          })}
        </div>
      )}

      {open && (
        <Dialog title={open.name} onClose={() => setOpen(null)} size={{ w: 760, h: 560 }}>
          <Markdown text={open.text} />
        </Dialog>
      )}
    </section>
  );
}
