//! Room offset on the TARMAC fixture (skips when `fixtures/` is absent).

use std::path::PathBuf;
use std::time::Instant;

use apc_media::{sync::measure_offset, AudioAnalysis};

#[test]
fn tarmac_room_runs_a_quarter_second_late() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tarmac-ep2");
    let paul = dir.join("Paul-9min_v001_.mp3");
    let room = dir.join("Paul-Room-9min_v001_.mp3");
    if !paul.exists() || !room.exists() {
        eprintln!("skipped: fixture audio not present in {}", dir.display());
        return;
    }
    let t = Instant::now();
    let a = AudioAnalysis::compute(&paul).unwrap();
    let b = AudioAnalysis::compute(&room).unwrap();
    eprintln!("decoded both in {:?}; {} s", t.elapsed(), a.info.duration_s);
    assert!((a.info.duration_s - 782.0).abs() < 2.0);
    let r = measure_offset(&a, &b, 3.0);
    eprintln!("offset {:.3} s, confidence {:.2}", r.offset_s, r.confidence);
    assert!((r.offset_s - 0.25).abs() <= 0.1, "{r:?}");
}
