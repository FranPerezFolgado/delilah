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
