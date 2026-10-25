import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import "./Dialog.css";

export interface DialogProps {
  title: string;
  onClose: () => void;
  footer?: ReactNode;
  size?: { w: number; h: number };
  children?: ReactNode;
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** Modal dialog: labelled, traps Tab, closes on Esc, gives focus back to the opener. */
export function Dialog({ title, onClose, footer, size, children }: DialogProps) {
  const id = useId();
  const box = useRef<HTMLDivElement>(null);
  const opener = useRef<Element | null>(document.activeElement);

  useEffect(() => {
    const from = opener.current as HTMLElement | null;
    const el = box.current;
    if (el && !el.contains(document.activeElement)) (el.querySelector<HTMLElement>(FOCUSABLE) ?? el).focus();
    return () => from?.focus?.();
  }, []);

  const place = (el: HTMLDivElement | null) => {
    box.current = el;
    if (el && size) {
      el.style.width = `${size.w}px`;
      el.style.height = `${size.h}px`;
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    } else if (e.key === "Tab") {
      const items = [...(box.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])];
      if (items.length === 0) {
        e.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      const at = document.activeElement;
      if (e.shiftKey && (at === first || at === box.current)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && at === last) {
        e.preventDefault();
        first.focus();
      }
    }
  };

  return (
    <div className="scrim">
      <div ref={place} className="dialog" role="dialog" aria-modal="true" aria-labelledby={id} tabIndex={-1} onKeyDown={onKeyDown}>
        <h2 id={id} className="dialog-hd">{title}</h2>
        <div className="dialog-bd">{children}</div>
        {footer != null && <div className="dialog-ft">{footer}</div>}
      </div>
    </div>
  );
}
