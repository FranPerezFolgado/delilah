//! The desktop shell. Deliberately thin: the UI talks to `delilahd` over its HTTP and
//! WebSocket API, never through Tauri commands, so the shell could be swapped for
//! Electron without touching the UI.

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Delilah");
}
