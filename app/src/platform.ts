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
