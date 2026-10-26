import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./Menu.css";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  title?: string;
  shortcut?: string;
}

export interface MenuTriggerProps {
  ref: (el: HTMLElement | null) => void;
  onClick: () => void;
  onKeyDown: (e: KeyboardEvent) => void;
  "aria-haspopup": "menu";
  "aria-expanded": boolean;
}

export interface MenuProps {
  trigger: (props: MenuTriggerProps) => ReactNode;
  items: MenuItem[];
}

/** Popup menu: role menu/menuitem, ↑↓ Home End, Esc, focus returns to the trigger. */
export function Menu({ trigger, items }: MenuProps) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  const triggerEl = useRef<HTMLElement | null>(null);
  const list = useRef<HTMLDivElement>(null);
  // Rendered in a portal (fixed, under the trigger) so no panel or clipping ancestor covers it.
  const [pos, setPos] = useState<CSSProperties>({});

  const buttons = () => [...(list.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];

  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) triggerEl.current?.focus();
  };

  useLayoutEffect(() => {
    if (!open || !wrap.current) return;
    const r = wrap.current.getBoundingClientRect();
    const w = list.current?.offsetWidth ?? 180;
    const left = Math.max(4, Math.min(r.left, window.innerWidth - w - 4));
    setPos({ top: r.bottom + 4, left });
  }, [open]);

  useEffect(() => {
    if (open) buttons()[0]?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      const el = e.target as Node;
      if (!wrap.current?.contains(el) && !list.current?.contains(el)) setOpen(false);
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);

  const onKeyDown = (e: KeyboardEvent) => {
    const bs = buttons();
    const at = bs.indexOf(document.activeElement as HTMLButtonElement);
    const go = (i: number) => {
      e.preventDefault();
      bs[(i + bs.length) % bs.length]?.focus();
    };
    if (e.key === "ArrowDown") go(at + 1);
    else if (e.key === "ArrowUp") go(at < 0 ? -1 : at - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(bs.length - 1);
    else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close(true);
    } else if (e.key === "Tab") close(false);
  };

  return (
    <span className="menu-wrap" ref={wrap}>
      {trigger({
        ref: (el) => {
          triggerEl.current = el;
        },
        onClick: () => setOpen((o) => !o),
        onKeyDown: (e) => {
          if (e.key === "ArrowDown" && !open) {
            e.preventDefault();
            setOpen(true);
          }
        },
        "aria-haspopup": "menu",
        "aria-expanded": open,
      })}
      {open &&
        createPortal(
        <div className="menu" role="menu" ref={list} style={pos} onKeyDown={onKeyDown}>
          {items.map((it) => (
            <button
              key={it.label}
              type="button"
              role="menuitem"
              className={it.danger ? "danger" : undefined}
              disabled={it.disabled}
              title={it.title}
              onClick={() => {
                close(true);
                it.onSelect();
              }}
            >
              <span className="menu-label">{it.label}</span>
              {it.shortcut && <span className="menu-kbd">{it.shortcut}</span>}
            </button>
          ))}
        </div>,
        document.body,
      )}
    </span>
  );
}
