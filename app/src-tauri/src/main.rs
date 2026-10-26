//! The desktop shell. Deliberately thin: the UI talks to `delilahd` over its HTTP and
//! WebSocket API, never through Tauri commands, so the shell could be swapped for
//! Electron without touching the UI. It owns window behaviour and keeping the daemon
//! running (see `daemon.rs`).

mod daemon;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

const RESTART_DAEMON: &str = "daemon-restart";
const STOP_DAEMON: &str = "daemon-stop";
const SHOW_LOGS: &str = "show-logs";
const CHECK_UPDATES: &str = "check-updates";

fn main() {
    let builder = tauri::Builder::default()
        // Native folder picker, e.g. to add a project.
        .plugin(tauri_plugin_dialog::init())
        // Native notifications for what waits on you (see app/src/platform.ts).
        .plugin(tauri_plugin_notification::init())
        // Links in Markdown previews open in the default browser, not in the window.
        .plugin(tauri_plugin_opener::init())
        // Relaunch after installing an update.
        .plugin(tauri_plugin_process::init());
    // Dev and unpackaged builds never check for or install updates.
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .invoke_handler(tauri::generate_handler![reveal_path])
        .menu(menu)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id == SHOW_LOGS {
                // The viewer lives in the UI: bring the window and tell it.
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.eval("window.dispatchEvent(new Event('delilah:show-logs'))");
                }
                return;
            }
            if id == CHECK_UPDATES {
                // The UI owns the update flow (app/src/useUpdates.ts): bring the window and tell it.
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit_to("main", CHECK_UPDATES, ());
                return;
            }
            if id != RESTART_DAEMON && id != STOP_DAEMON {
                return;
            }
            // Development builds never touch launchd: a LaunchAgent there would point
            // at a build folder and fight scripts/dev-daemon.sh for the port.
            let result = if cfg!(debug_assertions) {
                Err(if daemon::english() {
                    "In development the app does not manage the daemon: start it with scripts/dev-daemon.sh.".into()
                } else {
                    "En desarrollo el daemon no lo gestiona la app: arráncalo con scripts/dev-daemon.sh.".into()
                })
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

/// Resolves `path` for «Mostrar en Finder»: canonicalised first (so `..` and symlinks are
/// resolved before anything is judged), it must exist and lie under the canonical `$HOME`.
fn validate_reveal(path: &str, home: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let home = home.canonicalize().map_err(|e| format!("home: {e}"))?;
    let real = std::path::Path::new(path)
        .canonicalize()
        .map_err(|e| format!("{path}: {e}"))?;
    if real.starts_with(&home) {
        Ok(real)
    } else {
        Err(format!("{path}: outside the home folder"))
    }
}

/// Reveals a path in Finder. Refuses anything that is not under the user's home.
#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let real = validate_reveal(&path, std::path::Path::new(&home))?;
    std::process::Command::new("open")
        .arg("-R")
        .arg(real)
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err(format!("open exited with {s}"))
            }
        })
}

/// The usual macOS menus, minus "Close Window": in a terminal app Cmd+W closes the
/// active tab, so the shortcut has to reach the UI (see `app/src/shortcuts.ts`).
/// Edit stays, or copy and paste would not work in the webview.
fn menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    // In the language picked in Settings, read when the app opens.
    let en = daemon::english();
    let l = |es: &'static str, english: &'static str| if en { english } else { es };
    let app_menu = Submenu::with_items(
        app,
        "Delilah",
        true,
        &[
            &PredefinedMenuItem::about(app, Some(l("Acerca de Delilah", "About Delilah")), None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                CHECK_UPDATES,
                l("Buscar actualizaciones…", "Check for updates…"),
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app,
                RESTART_DAEMON,
                l("Reiniciar el daemon", "Restart the daemon"),
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(
                app,
                STOP_DAEMON,
                l("Detener el daemon", "Stop the daemon"),
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some(l("Ocultar Delilah", "Hide Delilah")))?,
            &PredefinedMenuItem::hide_others(app, Some(l("Ocultar otros", "Hide Others")))?,
            &PredefinedMenuItem::show_all(app, Some(l("Mostrar todo", "Show All")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some(l("Salir de Delilah", "Quit Delilah")))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        l("Edición", "Edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(l("Deshacer", "Undo")))?,
            &PredefinedMenuItem::redo(app, Some(l("Rehacer", "Redo")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some(l("Cortar", "Cut")))?,
            &PredefinedMenuItem::copy(app, Some(l("Copiar", "Copy")))?,
            &PredefinedMenuItem::paste(app, Some(l("Pegar", "Paste")))?,
            &PredefinedMenuItem::select_all(app, Some(l("Seleccionar todo", "Select All")))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        l("Ventana", "Window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(l("Minimizar", "Minimize")))?,
            &PredefinedMenuItem::maximize(app, Some(l("Zoom", "Zoom")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(
                app,
                Some(l("Pantalla completa", "Enter Full Screen")),
            )?,
        ],
    )?;
    let help = Submenu::with_items(
        app,
        l("Ayuda", "Help"),
        true,
        &[&MenuItem::with_id(
            app,
            SHOW_LOGS,
            l("Ver logs", "View logs"),
            true,
            Some("CmdOrCtrl+Shift+L"),
        )?],
    )?;
    Menu::with_items(app, &[&app_menu, &edit, &window, &help])
}

#[cfg(test)]
mod tests {
    use super::validate_reveal;
    use std::fs;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("delilah-reveal-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn plain_path_under_home_passes() {
        let root = tmp("ok");
        let home = root.join("home");
        fs::create_dir_all(home.join("data")).unwrap();
        let got = validate_reveal(home.join("data").to_str().unwrap(), &home).unwrap();
        assert!(got.ends_with("data"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dotdot_out_of_home_is_refused() {
        let root = tmp("dotdot");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join("other")).unwrap();
        let p = format!("{}/../other", home.display());
        assert!(validate_reveal(&p, &home).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn symlink_pointing_outside_home_is_refused() {
        let root = tmp("link");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        std::os::unix::fs::symlink(root.join("outside"), home.join("link")).unwrap();
        assert!(validate_reveal(home.join("link").to_str().unwrap(), &home).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_path_is_refused() {
        let root = tmp("missing");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        assert!(validate_reveal(home.join("nope").to_str().unwrap(), &home).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
