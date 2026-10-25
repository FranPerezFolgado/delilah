import { useEffect, useSyncExternalStore } from "react";
import { t } from "../../i18n";
import "./Toast.css";

export interface ToastInput {
  text: string;
  action?: { label: string; run: () => void };
  /** ms before an info toast goes away; default 6000. */
  ttl?: number;
  /** Errors and "te espera" stay until dismissed. */
  kind?: "info" | "error" | "wait";
}

export interface ToastItem extends ToastInput {
  id: number;
}

const MAX = 3;
const DEFAULT_TTL = 6000;
let items: ToastItem[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export function dismissToast(id: number) {
  items = items.filter((i) => i.id !== id);
  emit();
}
export function clearToasts() {
  items = [];
  emit();
}
export function pushToast(input: ToastInput): number {
  const id = nextId++;
  items = [...items, { ...input, id }].slice(-MAX);
  emit();
  return id;
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};
const snapshot = () => items;

/** `toast({text, action?, ttl?, kind?})` from any component. */
export function useToast() {
  return pushToast;
}

function ToastView({ item }: { item: ToastItem }) {
  const persists = (item.kind ?? "info") !== "info";
  useEffect(() => {
    if (persists) return;
    const h = setTimeout(() => dismissToast(item.id), item.ttl ?? DEFAULT_TTL);
    return () => clearTimeout(h);
  }, [item.id, item.ttl, persists]);
  return (
    <div className={`toast toast-${item.kind ?? "info"}`} role="status">
      <span className="toast-text">{item.text}</span>
      {item.action && (
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            item.action!.run();
            dismissToast(item.id);
          }}
        >
          {item.action.label}
        </button>
      )}
      <button type="button" className="btn btn-ghost" aria-label={t("Cerrar")} onClick={() => dismissToast(item.id)}>
        ×
      </button>
    </div>
  );
}

/** The one toast stack (bottom-left of main); mount it once. */
export function ToastHost() {
  const list = useSyncExternalStore(subscribe, snapshot);
  if (list.length === 0) return null;
  return (
    <div className="toast-host">
      {list.map((i) => (
        <ToastView key={i.id} item={i} />
      ))}
    </div>
  );
}
