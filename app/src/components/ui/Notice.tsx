import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { t } from "../../i18n";
import { IconButton } from "./Button";
import "./Notice.css";

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** The one close control for notifications. */
export function CloseButton({ onClick, className }: { onClick: () => void; className?: string }) {
  return <IconButton icon="close" label={t("Cerrar")} className={`ntc-x ${className ?? ""}`.trim()} onClick={onClick} />;
}

/** Focus the first focusable element that follows `root` in document order. */
function focusAfter(root: HTMLElement) {
  const all = Array.from(document.querySelectorAll<HTMLElement>(FOCUSABLE));
  const next = all.find(
    (el) =>
      !root.contains(el) &&
      root.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING &&
      !el.closest("[inert]") &&
      !el.closest("[hidden]"),
  );
  next?.focus();
}

/** A bar/banner: the caller's content and actions, then the CloseButton, last. */
export function Notice({ className, role, onClose, children }: { className?: string; role?: "status" | "alert"; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const close = () => {
    const root = ref.current;
    const hadFocus = !!root && root.contains(document.activeElement);
    if (root && hadFocus) focusAfter(root);
    onClose();
  };
  return (
    <div ref={ref} role={role} className={`ntc ${className ?? ""}`.trim()}>
      {children}
      <CloseButton onClick={close} />
    </div>
  );
}

/** Per-id dismissals that last while each id's signature is unchanged. */
export function useDismissals(current: Record<string, string>): { hidden: (id: string) => boolean; dismiss: (id: string) => void } {
  const [dismissed, setDismissed] = useState<Record<string, string>>({});
  const key = JSON.stringify(current);
  const currentRef = useRef(current);
  currentRef.current = current;

  // Forget a dismissal once its id is gone or its signature has changed.
  useEffect(() => {
    const cur = currentRef.current;
    setDismissed((prev) => {
      const ids = Object.keys(prev);
      const keep = ids.filter((id) => cur[id] === prev[id]);
      if (keep.length === ids.length) return prev;
      return Object.fromEntries(keep.map((id) => [id, prev[id]]));
    });
  }, [key]);

  const hidden = (id: string) => id in current && dismissed[id] === current[id];
  const dismiss = useCallback((id: string) => {
    const sig = currentRef.current[id];
    if (sig !== undefined) setDismissed((prev) => ({ ...prev, [id]: sig }));
  }, []);
  return { hidden, dismiss };
}

/** One notice; `null` = not showing (forgets the dismissal). */
export function useDismissal(sig: string | null): [hidden: boolean, dismiss: () => void] {
  const { hidden, dismiss } = useDismissals(sig === null ? {} : { n: sig });
  const d = useCallback(() => dismiss("n"), [dismiss]);
  return [hidden("n"), d];
}
