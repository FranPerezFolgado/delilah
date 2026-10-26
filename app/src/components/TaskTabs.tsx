import { useCallback, useEffect, useRef, useState } from "react";
import { t } from "../i18n";
import { keyOf } from "../shortcuts";
import type { SessionDot } from "../vocab";
import { Count, Icon, Menu } from "./ui";
import "./TaskTabs.css";

export interface TaskTabItem {
  /** Unique: a path for files, a session id, or "task" | "project" | "changes" | "pr". */
  id: string;
  kind: "task" | "project" | "session" | "file" | "changes" | "pr";
  label: string;
  /** Session tabs. */
  dot?: SessionDot;
  /** File tabs with unsaved edits. */
  dirty?: boolean;
  /** Cambios: changed files. */
  count?: number;
  /** Files and Cambios can be closed. */
  closable?: boolean;
  title?: string;
}

export interface TaskTabsProps {
  tabs: TaskTabItem[];
  active: string;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onNewSession: (kind: "claude" | "terminal") => void;
}

/** Ids of the tabs that are not fully inside the scroll box. */
export function hiddenTabs(box: HTMLElement): string[] {
  if (!box.clientWidth) return [];
  const left = box.scrollLeft;
  const right = left + box.clientWidth;
  const out: string[] = [];
  box.querySelectorAll<HTMLElement>("[data-tab]").forEach((el) => {
    if (el.offsetLeft < left - 1 || el.offsetLeft + el.offsetWidth > right + 1) out.push(el.dataset.tab!);
  });
  return out;
}

export function TaskTabs({ tabs, active, onSelect, onClose, onNewSession }: TaskTabsProps) {
  const box = useRef<HTMLDivElement>(null);
  const [hidden, setHidden] = useState<string[]>([]);

  const measure = useCallback(() => {
    if (!box.current) return;
    const h = hiddenTabs(box.current);
    setHidden((old) => (old.join("\0") === h.join("\0") ? old : h));
  }, []);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    measure();
    el.addEventListener("scroll", measure, { passive: true });
    window.addEventListener("resize", measure);
    return () => {
      el.removeEventListener("scroll", measure);
      window.removeEventListener("resize", measure);
    };
  }, [measure, tabs]);

  // The open tab scrolls into view.
  useEffect(() => {
    const el = [...(box.current?.querySelectorAll<HTMLElement>("[data-tab]") ?? [])].find((x) => x.dataset.tab === active);
    el?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [active]);

  const reveal = (id: string) => {
    onSelect(id);
  };

  return (
    <div className="ttabs">
      <div className="ttabs-scroll" role="tablist" aria-label={t("Pestañas de la tarea")} ref={box}>
        {tabs.map((tab) => {
          const on = tab.id === active;
          return (
            <div key={tab.id} data-tab={tab.id} className={`ttab ${tab.kind}${on ? " on" : ""}${tab.closable ? " closable" : ""}`}>
              <button
                type="button"
                role="tab"
                aria-selected={on}
                className="ttab-main"
                title={tab.title ?? tab.label}
                onClick={() => onSelect(tab.id)}
              >
                {tab.kind === "session" && <span className={`ttab-dot ${tab.dot ?? "neutral"}`} aria-hidden="true" />}
                <span className="ttab-label">{tab.label}</span>
                {tab.kind === "changes" && <Count n={tab.count ?? 0} />}
              </button>
              {tab.closable && (
                <button type="button" className="ttab-x" aria-label={t("Cerrar {name}", { name: tab.label })} onClick={() => onClose(tab.id)}>
                  {tab.dirty ? <span className="ttab-dirty" title={t("sin guardar")} /> : <Icon name="close" />}
                </button>
              )}
            </div>
          );
        })}
      </div>
      <div className="ttabs-end">
        {hidden.length > 0 && (
          <Menu
            trigger={(p) => (
              <button type="button" className="ttabs-more" {...p}>
                {t("›› {n} más", { n: hidden.length })}
              </button>
            )}
            items={hidden.map((id) => ({ label: tabs.find((x) => x.id === id)?.label ?? id, onSelect: () => reveal(id) }))}
          />
        )}
        <Menu
          trigger={(p) => (
            <button type="button" className="ttabs-new" {...p}>
              {t("+ Sesión")}
            </button>
          )}
          items={[
            { label: t("Claude"), shortcut: keyOf("new-claude"), onSelect: () => onNewSession("claude") },
            { label: t("Terminal"), shortcut: keyOf("new-terminal"), onSelect: () => onNewSession("terminal") },
          ]}
        />
      </div>
    </div>
  );
}
