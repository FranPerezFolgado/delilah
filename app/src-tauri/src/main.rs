//! The desktop shell. Deliberately thin: the UI talks to `delilahd` over its HTTP and
//! WebSocket API, never through Tauri commands, so the shell could be swapped for
//! Electron without touching the UI. Window behaviour is the one thing it owns.

use tauri::{Manager, RunEvent, WindowEvent};

fn main() {
    tauri::Builder::default()
        .on_window_event(|window, event| {
            // macOS convention: closing the window (Cmd+W, the red button) keeps the
            // app running in the Dock; Cmd+Q quits. The agents live in the daemon
            // either way.
            if let WindowEvent::CloseRequested { api, .. } = event
                && cfg!(target_os = "macos")
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Delilah")
        .run(|app, event| {
            // Clicking the Dock icon brings the hidden window back.
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { has_visible_windows: false, .. } = event
                && let Some(window) = app.get_webview_window("main")
            {
                let _ = window.show();
                let _ = window.set_focus();
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
