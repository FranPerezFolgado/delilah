//! The desktop shell. Deliberately thin: the UI talks to `delilahd` over its HTTP and
//! WebSocket API, never through Tauri commands, so the shell could be swapped for
//! Electron without touching the UI. It owns window behaviour and keeping the daemon
//! running (see `daemon.rs`).

mod daemon;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, RunEvent, Runtime, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

const RESTART_DAEMON: &str = "daemon-restart";
const STOP_DAEMON: &str = "daemon-stop";

fn main() {
    tauri::Builder::default()
        // Native folder picker, e.g. to add a project.
        .plugin(tauri_plugin_dialog::init())
        .menu(menu)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id != RESTART_DAEMON && id != STOP_DAEMON {
                return;
            }
            // Development builds never touch launchd: a LaunchAgent there would point
            // at a build folder and fight scripts/dev-daemon.sh for the port.
            let result = if cfg!(debug_assertions) {
                Err("En desarrollo el daemon no lo gestiona la app: arráncalo con scripts/dev-daemon.sh.".into())
            } else if id == RESTART_DAEMON {
                daemon::ensure_running().and_then(|()| daemon::restart())
            } else {
                daemon::stop()
            };
            if let Err(e) = result {
                app.dialog()
                    .message(e)
                    .title("Delilah")
                    .kind(MessageDialogKind::Error)
                    .show(|_| {});
            }
        })
        .setup(|_app| {
            // Release builds start the bundled daemon; in development it runs on its own.
            if cfg!(all(target_os = "macos", not(debug_assertions))) {
                std::thread::spawn(|| {
                    if let Err(e) = daemon::ensure_running() {
                        eprintln!("delilahd: {e}");
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // macOS convention: the red button hides the window and the app stays in
            // the Dock; Cmd+Q quits. The agents live in the daemon either way.
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

/// The usual macOS menus, minus "Close Window": in a terminal app Cmd+W closes the
/// active tab, so the shortcut has to reach the UI (see `app/src/shortcuts.ts`).
/// Edit stays, or copy and paste would not work in the webview.
fn menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let app_menu = Submenu::with_items(
        app,
        "Delilah",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("Acerca de Delilah"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, RESTART_DAEMON, "Reiniciar el daemon", true, None::<&str>)?,
            &MenuItem::with_id(app, STOP_DAEMON, "Detener el daemon", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("Ocultar Delilah"))?,
            &PredefinedMenuItem::hide_others(app, Some("Ocultar otros"))?,
            &PredefinedMenuItem::show_all(app, Some("Mostrar todo"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some("Salir de Delilah"))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edición",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("Deshacer"))?,
            &PredefinedMenuItem::redo(app, Some("Rehacer"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some("Cortar"))?,
            &PredefinedMenuItem::copy(app, Some("Copiar"))?,
            &PredefinedMenuItem::paste(app, Some("Pegar"))?,
            &PredefinedMenuItem::select_all(app, Some("Seleccionar todo"))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Ventana",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Minimizar"))?,
            &PredefinedMenuItem::maximize(app, Some("Zoom"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, Some("Pantalla completa"))?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &edit, &window])
}
