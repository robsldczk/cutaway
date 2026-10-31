//! Tauri shell: forwards every UI request to the shared `apc-app` session. The UI never
//! touches audio or state directly; it calls `rpc(method, params)`.

use std::sync::Mutex;

use serde_json::Value;
use tauri::{Manager, State};

use apc_app::error::ErrorBody;
use apc_app::Session;

struct AppSession(Mutex<Session>);

#[tauri::command]
async fn rpc(
    state: State<'_, AppSession>,
    method: String,
    params: Option<Value>,
) -> Result<Value, ErrorBody> {
    let params = params.unwrap_or(Value::Null);
    let mut s = state.0.lock().map_err(|_| ErrorBody {
        code: "error".into(),
        message: "session lock poisoned".into(),
    })?;
    s.call(&method, &params).map_err(|e| ErrorBody::from(&e))
}

fn initial_session() -> Session {
    let arg = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .or_else(|| std::env::var("CUTAWAY_PROJECT").ok());
    match arg {
        Some(p) => Session::with_project(std::path::Path::new(&p), "ui").unwrap_or_else(|e| {
            eprintln!("could not open {p}: {e}");
            Session::new("ui")
        }),
        None => Session::new("ui"),
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppSession(Mutex::new(initial_session())))
        .invoke_handler(tauri::generate_handler![rpc])
        .build(tauri::generate_context!());
    let app = match app {
        Ok(a) => a,
        Err(err) => {
            eprintln!("{} failed to start: {err}", apc_core::brand::DISPLAY_NAME);
            std::process::exit(1);
        }
    };
    app.run(|_handle, _event| {
        // Double-clicking an episode file in Finder while the app runs.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if let tauri::RunEvent::Opened { urls } = &_event {
            if let Some(path) = urls.iter().find_map(|u| u.to_file_path().ok()) {
                if let Some(state) = _handle.try_state::<AppSession>() {
                    if let Ok(mut s) = state.0.lock() {
                        if let Ok(n) = Session::with_project(&path, "ui") {
                            *s = n;
                        }
                    }
                }
            }
        }
    });
}
