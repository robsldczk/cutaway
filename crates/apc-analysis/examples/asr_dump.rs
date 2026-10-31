//! Dumps a Parakeet transcript as JSON (word times, segments) for comparing ASR runs.
//! `cargo run --release -p apc-analysis --features parakeet --example asr_dump -- <audio> <out.json>`

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(audio), Some(out)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: asr_dump <audio> <out.json>");
        std::process::exit(2);
    };
    #[cfg(feature = "parakeet")]
    {
        use apc_analysis::asr::{AsrProvider, AsrRequest, ModelManager};
        let asr = apc_analysis::parakeet::ParakeetAsr::new(ModelManager::new(
            ModelManager::default_root(),
        ));
        let t0 = std::time::Instant::now();
        let req = AsrRequest {
            audio: audio.into(),
            sidecar: None,
            hotwords: vec![],
            language: "en".into(),
        };
        match asr.transcribe(&req) {
            Ok(t) => {
                let json = serde_json::to_string_pretty(&t).unwrap_or_default();
                if let Err(e) = std::fs::write(out, json) {
                    eprintln!("cannot write {out}: {e}");
                    std::process::exit(1);
                }
                eprintln!(
                    "{} words, {} segments in {:?}",
                    t.words.len(),
                    t.segments.len(),
                    t0.elapsed()
                );
            }
            Err(e) => {
                eprintln!("ASR failed: {e}");
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(feature = "parakeet"))]
    {
        let _ = (audio, out);
        eprintln!("build with --features parakeet");
        std::process::exit(2);
    }
}
