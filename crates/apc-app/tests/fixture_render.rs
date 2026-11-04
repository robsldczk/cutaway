//! M5 on the TARMAC fixture: the offline render of S1 matches the layout length (±10 ms).
//! Also writes the fixture exports to `fixtures/tarmac-ep2/out/` for the manual Resolve check.

use std::path::PathBuf;

use serde_json::json;

#[test]
fn tarmac_s1_render_length_and_exports() {
    std::env::set_var("CUTAWAY_AI_PROVIDER", "mock");
    std::env::set_var("CUTAWAY_NO_AUDIO", "1");
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tarmac-ep2");
    if !fx.join("Paul-9min_v001_.mp3").exists() {
        eprintln!("skipped: fixture not present");
        return;
    }
    let out = fx.join("out");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut svc = apc_app::Service::create(&out, "Ep02", None, "test").unwrap();
    svc.import_script(
        &fx.join("tarmac-episode-2_rob-test.fountain"),
        Some("TARMAC Ep2"),
    )
    .unwrap();
    svc.import_audio(
        &[
            fx.join("Paul-9min_v001_.mp3"),
            fx.join("Paul-Room-9min_v001_.mp3"),
        ],
        false,
    )
    .unwrap();
    svc.analyze(&|_| {}).unwrap();

    let s1 = svc.doc().find_scene("1").unwrap().clone();
    let layout = apc_core::timeline::layout_scene(svc.doc(), &s1, &svc.config().pacing());
    assert!(layout.duration_s > 10.0);
    let (wav, len) = apc_export::bounce::bounce_scene(
        svc.doc(),
        &svc.paths,
        "1",
        apc_export::bounce::Format::Wav,
        &svc.paths.exports,
    )
    .unwrap();
    eprintln!(
        "S1: layout {:.3} s, render {:.3} s -> {}",
        layout.duration_s,
        len,
        wav.display()
    );
    // the voices are in the room, so it rings on a little past the last line
    let tail = apc_audio::RoomShape::new(s1.room.size, s1.room.damping, s1.room.wet).tail_s();
    assert!(len >= layout.duration_s - 0.010);
    assert!(len <= layout.duration_s + tail + 0.010);

    for (m, p) in [
        ("export_fcpxml", json!({})),
        ("export_fcpxml", json!({"scene": "1"})),
        ("export_cut_list", json!({"format": "md"})),
        ("export_cut_list", json!({})),
        ("export_srt", json!({})),
    ] {
        let r = svc.call(m, &p).unwrap();
        eprintln!("{m}: {}", r["path"]);
    }
}
