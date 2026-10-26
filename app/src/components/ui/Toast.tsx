import { useEffect, useState, useSyncExternalStore } from "react";
import { t } from "../../i18n";
import "./Toast.css";

export interface ToastInput {
  text: string;
  action?: { label: string; run: () => void };
  /** ms before an info toast goes away; default 6000. */
  ttl?: number;
  /** Errors and "te espera" stay until dismissed. */
  kind?: "info" | "error" | "wait";
  /** In-place identity: pushing the same key updates the toast; keyed toasts never expire or get evicted. */
  key?: string;
  /** Called when the user closes the toast (×), not on expiry or programmatic dismissal. */
  onDismiss?: () => void;
}

export interface ToastItem extends ToastInput {
  id: number;
  count: number;
  aliases: number[];
  /** Absolute expiry (ms epoch) while running; unset when paused or persistent. */
  deadline?: number;
  /** Time left (ms) while paused. */
  remaining?: number;
}

export const WAITING_TOAST_KEY = "waiting";

const MAX = 3;
const DEFAULT_TTL = 6000;
let items: ToastItem[] = [];
let nextId = 1;
let paused = false;
/** Text for the screen-reader live region: last push, merge or keyed update. */
let lastAnnounced = "";
let timer: ReturnType<typeof setTimeout> | undefined;
const listeners = new Set<() => void>();

const expires = (i: ToastInput) => (i.kind ?? "info") === "info" && !i.key;

function schedule() {
  if (timer !== undefined) clearTimeout(timer);
  timer = undefined;
  if (paused) return;
  const deadlines = items.flatMap((i) => (i.deadline !== undefined ? [i.deadline] : []));
  if (deadlines.length === 0) return;
  const delay = Math.max(0, Math.min(...deadlines) - Date.now());
  timer = setTimeout(() => {
    timer = undefined;
    const now = Date.now();
    const next = items.filter((i) => i.deadline === undefined || i.deadline > now);
    if (next.length !== items.length) {
      items = next;
      emit();
    } else schedule();
  }, delay);
}
function emit() {
  schedule();
  listeners.forEach((l) => l());
}

const find = (id: number) => items.find((i) => i.id === id || i.aliases.includes(id));

export function dismissToast(id: number) {
  const hit = find(id);
  if (!hit) return;
  items = items.filter((i) => i !== hit);
  emit();
}
export function dismissToastByKey(key: string) {
  const next = items.filter((i) => i.key !== key);
  if (next.length === items.length) return;
  items = next;
  emit();
}
/** The user closed it: tell the owner, then remove. */
function closeToast(id: number) {
  const hit = find(id);
  if (!hit) return;
  hit.onDismiss?.();
  dismissToast(id);
}
export function clearToasts() {
  items = [];
  paused = false;
  lastAnnounced = "";
  emit();
}
/** Freezes (or resumes) every auto-dismiss timer at once. */
export function setToastsPaused(p: boolean) {
  if (p === paused) return;
  paused = p;
  const now = Date.now();
  items = items.map((i) => {
    if (p) {
      if (i.deadline === undefined) return i;
      const { deadline, ...rest } = i;
      return { ...rest, remaining: Math.max(0, deadline - now) };
    }
    if (i.remaining === undefined) return i;
    const { remaining, ...rest } = i;
    return { ...rest, deadline: now + remaining };
  });
  emit();
}

/** Drops toasts beyond MAX, sparing the just-pushed one (`keepId`) while anything else can go. */
function evict(keepId: number) {
  while (items.length > MAX) {
    const others = items.filter((i) => i.id !== keepId);
    const victim =
      others.find((i) => (i.kind ?? "info") === "info" && !i.action && !i.key) ??
      others.find((i) => !i.key) ??
      items.find((i) => !i.key);
    if (!victim) return;
    items = items.filter((i) => i !== victim);
  }
}

export function pushToast(input: ToastInput): number {
  const ttl = input.ttl ?? DEFAULT_TTL;
  const timing = (i: ToastInput) =>
    !expires(i) ? {} : paused ? { remaining: ttl } : { deadline: Date.now() + ttl };
  if (input.key) {
    const hit = items.find((i) => i.key === input.key);
    if (hit) {
      items = items.map((i) =>
        i === hit ? { ...input, id: hit.id, count: hit.count, aliases: hit.aliases } : i,
      );
      lastAnnounced = input.text;
      emit();
      return hit.id;
    }
  }
  const id = nextId++;
  if (!input.action) {
    const kind = input.kind ?? "info";
    const twin = items.find((i) => !i.key && !i.action && (i.kind ?? "info") === kind && i.text === input.text);
    if (twin) {
      const { deadline: _d, remaining: _r, ...rest } = twin;
      const merged = { ...rest, count: twin.count + 1, aliases: [...twin.aliases, id], ...timing(twin) };
      items = [...items.filter((i) => i !== twin), merged];
      lastAnnounced = `${input.text} ×${merged.count}`;
      emit();
      return id;
    }
  }
  items = [...items, { ...input, id, count: 1, aliases: [], ...timing(input) }];
  evict(id);
  lastAnnounced = input.text;
  emit();
  return id;
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};
const snapshot = () => items;
const announcedSnapshot = () => lastAnnounced;

/** `toast({text, action?, ttl?, kind?})` from any component. */
export function useToast() {
  return pushToast;
}

function ToastView({ item }: { item: ToastItem }) {
  return (
    <div className={`toast toast-${item.kind ?? "info"}`}>
      <span className="toast-text">{item.text}</span>
      {item.count > 1 && (
        <span className="toast-count">{t("×{n}", { n: item.count > 99 ? "99+" : item.count })}</span>
      )}
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
      <button type="button" className="btn btn-ghost" aria-label={t("Cerrar")} onClick={() => closeToast(item.id)}>
        ×
      </button>
    </div>
  );
}

/** The user closed everything: tell each owner, then empty the stack. */
function closeAll() {
  const all = items;
  all.forEach((i) => i.onDismiss?.());
  items = items.filter((i) => !all.includes(i));
  emit();
}

/** The one toast stack (bottom-left of main); mount it once. */
export function ToastHost() {
  const list = useSyncExternalStore(subscribe, snapshot);
  const announced = useSyncExternalStore(subscribe, announcedSnapshot);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [pinned, setPinned] = useState(false);
  const expanded = hovered || focused || pinned;
  useEffect(() => setToastsPaused(expanded), [expanded]);
  useEffect(() => () => setToastsPaused(false), []);
  if (list.length === 0) return null;
  const more = list.length - 1;
  const shown = expanded ? list : list.slice(-1);
  return (
    <div
      className={`toast-host${expanded ? " toast-expanded" : ""}`}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => {
        setHovered(false);
        if (!focused) setPinned(false);
      }}
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
        setFocused(false);
        if (!hovered) setPinned(false);
      }}
    >
      <div role="status" aria-live="polite" className="toast-sr">
        {announced}
      </div>
      {shown.map((i) => (
        <ToastView key={i.id} item={i} />
      ))}
      {more > 0 && !expanded && (
        <button
          type="button"
          className="toast-more"
          aria-expanded="false"
          aria-label={t("Mostrar {n} avisos más", { n: more })}
          onClick={() => setPinned(true)}
        >
          {t("+{n}", { n: more })}
        </button>
      )}
      {more > 0 && expanded && (
        <button type="button" className="toast-clear" onClick={closeAll}>
          {t("Cerrar todo")}
        </button>
      )}
    </div>
  );
}
