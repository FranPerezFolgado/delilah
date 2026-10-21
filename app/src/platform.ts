// The few things that need the desktop shell rather than the daemon. Kept here so
// another shell (e.g. Electron) only has to reimplement this file.

/** Running inside the Tauri desktop app (not a plain browser). */
export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Native "choose a folder" dialog. Resolves to the folder, or null if cancelled. */
export async function pickFolder(title: string): Promise<string | null> {
  if (!isDesktop) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({ directory: true, multiple: false, title });
  return typeof chosen === "string" ? chosen : null;
}

/** The count on the Dock icon; 0 clears it. */
export async function setBadge(count: number): Promise<void> {
  if (!isDesktop) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().setBadgeCount(count > 0 ? count : undefined);
}

/** A native notification. macOS asks once for permission; if it is refused, nothing shows. */
export async function notify(title: string, body: string): Promise<void> {
  if (!isDesktop) return;
  const n = await import("@tauri-apps/plugin-notification");
  let granted = await n.isPermissionGranted();
  if (!granted) granted = (await n.requestPermission()) === "granted";
  if (granted) n.sendNotification({ title, body });
}

/** Opens a web or mail link in the system's browser or mail app; outside the desktop
 *  app, in a new tab. */
export async function openExternal(url: string): Promise<void> {
  if (!isDesktop) {
    window.open(url, "_blank", "noopener");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

/** A click in the playground window asking the main window to open a task's terminal. */
export interface OpenTaskRequest {
  task: string;
  session: string;
}

/** Both fields non-empty strings of at most 200 chars; anything else is dropped. */
function validRequest(d: unknown): OpenTaskRequest | null {
  const { task, session } = (d ?? {}) as Record<string, unknown>;
  const ok = (v: unknown): v is string => typeof v === "string" && v.length > 0 && v.length <= 200;
  return ok(task) && ok(session) ? { task, session } : null;
}

const OPEN_TASK_EVENT = "delilah:open-task";

/** Opens the playground in a window of its own, or focuses it if already open. In the
 *  browser, a named tab that is reused. */
export function openPlaygroundWindow(): Promise<void> {
  inFlight ??= openPlayground().finally(() => {
    inFlight = null;
  });
  return inFlight;
}

let inFlight: Promise<void> | null = null;

async function openPlayground(): Promise<void> {
  if (!isDesktop) {
    const url = `${location.href.split("#")[0]}#playground`;
    window.open(url, "delilah-playground")?.focus();
    return;
  }
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const existing = await WebviewWindow.getByLabel("playground");
  if (existing) {
    await existing.unminimize();
    await existing.show();
    await existing.setFocus();
    return;
  }
  const win = new WebviewWindow("playground", {
    url: "index.html#playground",
    title: "Delilah — Playground",
    width: 1100,
    height: 700,
  });
  void win.once("tauri://error", (e) => console.warn("playground window failed to open", e.payload));
}

/** From the playground window: asks the main window to come to the front on that task. */
export async function requestOpenTask(p: OpenTaskRequest): Promise<void> {
  if (!isDesktop) {
    const ch = new BroadcastChannel("delilah");
    ch.postMessage({ type: "open-task", task: p.task, session: p.session });
    ch.close();
    window.opener?.focus();
    return;
  }
  const { emitTo } = await import("@tauri-apps/api/event");
  const { Window } = await import("@tauri-apps/api/window");
  await emitTo("main", OPEN_TASK_EVENT, p);
  const main = await Window.getByLabel("main");
  if (main) {
    await main.unminimize();
    await main.show();
    await main.setFocus();
  }
}

/** In the main window: calls `cb` for every request from the playground window.
 *  Resolves to the unsubscribe function. */
export async function onOpenTask(cb: (p: OpenTaskRequest) => void): Promise<() => void> {
  if (!isDesktop) {
    const ch = new BroadcastChannel("delilah");
    ch.onmessage = (e: MessageEvent) => {
      if (e.data?.type !== "open-task") return;
      const p = validRequest(e.data);
      if (p) cb(p);
    };
    return () => ch.close();
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<OpenTaskRequest>(OPEN_TASK_EVENT, (e) => {
    const p = validRequest(e.payload);
    if (p) cb(p);
  });
}
