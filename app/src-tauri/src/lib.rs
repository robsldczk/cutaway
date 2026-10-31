//! Tauri shell: forwards UI requests to the shared `apc-app` service.

pub fn run() {
    let result = tauri::Builder::default().run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("{} failed to start: {err}", apc_core::brand::DISPLAY_NAME);
        std::process::exit(1);
    }
}
