use std::process::Command;

/// The commit the app is built from, for the About box (D162). The UI gets it from vite.
fn commit() -> String {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    match git(&["rev-parse", "--short=7", "HEAD"]) {
        Some(hash) => {
            let modified = git(&["status", "--porcelain", "--untracked-files=no"])
                .is_some_and(|s| !s.is_empty());
            if modified {
                format!("{hash}-modified")
            } else {
                hash
            }
        }
        None => "unknown".into(),
    }
}

fn main() {
    println!("cargo:rustc-env=CUTAWAY_COMMIT={}", commit());
    // a new commit or a checkout changes HEAD; edits to tracked files change the index
    for f in ["../../.git/HEAD", "../../.git/index"] {
        if std::path::Path::new(f).exists() {
            println!("cargo:rerun-if-changed={f}");
        }
    }
    tauri_build::build()
}
