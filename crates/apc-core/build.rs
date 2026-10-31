//! Reads `brand.json` at the repo root and generates `brand.rs` constants.
use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let brand_path = manifest.join("../../brand.json");
    println!("cargo:rerun-if-changed={}", brand_path.display());
    let text = fs::read_to_string(&brand_path).expect("brand.json at the repo root");
    let v: serde_json::Value = serde_json::from_str(&text).expect("brand.json is valid JSON");
    let get = |k: &str| -> String {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_else(|| panic!("brand.json is missing \"{k}\""))
            .to_string()
    };
    let consts = [
        ("DISPLAY_NAME", "displayName"),
        ("SLUG", "slug"),
        ("CLI_NAME", "cliName"),
        ("EPISODE_EXT", "episodeExt"),
        ("SHOW_EXT", "showExt"),
        ("BUNDLE_ID", "bundleId"),
        ("TAGLINE", "tagline"),
    ];
    let mut out = String::from("// Generated from brand.json by build.rs. Do not edit.\n");
    for (name, key) in consts {
        out.push_str(&format!("pub const {name}: &str = {:?};\n", get(key)));
    }
    let dest = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("brand.rs");
    fs::write(dest, out).expect("write brand.rs");
}
