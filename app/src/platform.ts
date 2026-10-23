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

/** Updates only exist in the installed desktop app, not in `tauri dev` or a browser. */
export const canUpdate = isDesktop && !import.meta.env.DEV;

/** The installed app's version; null outside the desktop app. */
export async function appVersion(): Promise<string | null> {
  if (!isDesktop) return null;
  const { getVersion } = await import("@tauri-apps/api/app");
  return getVersion();
}

export interface FoundUpdate {
  version: string;
  notes: string;
  /** Downloads and installs; `total` is null when the size is unknown. */
  install(onProgress: (received: number, total: number | null) => void): Promise<void>;
}

/** Looks for a newer release. Null when up to date (or when updating is unavailable);
 *  throws on a network or manifest error. */
export async function checkForUpdate(): Promise<FoundUpdate | null> {
  if (!canUpdate) return null;
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    notes: update.body ?? "",
    async install(onProgress) {
      let received = 0;
      let total: number | null = null;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? null;
          onProgress(0, total);
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          onProgress(received, total);
        }
      });
    },
  };
}

/** Restarts the app (after an update was installed). */
export async function relaunch(): Promise<void> {
  if (!canUpdate) return;
  const p = await import("@tauri-apps/plugin-process");
  await p.relaunch();
}

/** Calls `cb` when the menu item "Buscar actualizaciones…" is chosen; returns the unlisten. */
export async function onCheckUpdatesMenu(cb: () => void): Promise<() => void> {
  if (!canUpdate) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen("check-updates", () => cb());
}
