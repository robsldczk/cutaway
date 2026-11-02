//! FCPXML structure (and DTD validation when Final Cut Pro's DTD is installed), SRT and
//! cut list on a small synthetic episode.

use std::collections::HashMap;
use std::path::Path;

use apc_core::*;
use apc_export::fcpxml::{self, parse_rt};
use apc_export::{cutlist, srt, EpisodeTimeline, ExportScope};

fn word(raw: &str, t0: f64, t1: f64, i: usize) -> TakeWord {
    TakeWord {
        w: raw.to_lowercase(),
        raw: raw.into(),
        t0,
        t1,
        script_idx: Some(i),
    }
}

fn take(id: &str, line: &str, src: &str, no: u32, t0: f64, t1: f64, words: Vec<TakeWord>) -> Take {
    Take {
        id: id.into(),
        line_id: line.into(),
        source_id: src.into(),
        take_no: no,
        take_type: TakeType::Full,
        t0,
        t1,
        words,
        score: 100.0,
        heard: String::new(),
        lufs: Some(-20.0),
        db: -18.0,
        partial: false,
        pick: 0,
        favorite: false,
        favorite_origin: None,
        direction: "quieter".into(),
        feedback: "liked it".into(),
        direction_raw: String::new(),
        feedback_raw: String::new(),
        tags: vec![],
        note_ids: vec![],
        confidence: 1.0,
        needs_rematch: false,
        ai_flag: None,
        origin: Default::default(),
        read_as: None,
    }
}

fn setup(dir: &Path) -> (apc_core::project::EpisodePaths, Document) {
    let (paths, _store) = apc_core::project::create_episode(dir, "Ep & Test", None).unwrap();
    let sr = 48000u32;
    let tone: Vec<f32> = (0..sr * 6)
        .map(|i| 0.3 * (i as f32 * 300.0 * std::f32::consts::TAU / sr as f32).sin())
        .collect();
    std::fs::create_dir_all(paths.audio.join("Session 1")).unwrap();
    for n in ["Paul.wav", "Sara.wav"] {
        apc_media::wav::write_wav(&paths.audio.join("Session 1").join(n), sr, 1, &tone).unwrap();
    }
    std::fs::create_dir_all(paths.library.join("SFX/Misc")).unwrap();
    apc_media::wav::write_wav(
        &paths.library.join("SFX/Misc/door.wav"),
        sr,
        1,
        &tone[..sr as usize],
    )
    .unwrap();
    let mut d = Document::default();
    for (id, ch, name) in [("src_p", "PAUL", "Paul.wav"), ("src_s", "SARA", "Sara.wav")] {
        d.sources.insert(
            id.into(),
            SourceFile {
                id: id.into(),
                path: format!("Audio/Session 1/{name}"),
                original_name: name.into(),
                stored: Stored::Copied,
                hash: "h".into(),
                size: 1,
                role: Role::Character,
                character: Some(ch.into()),
                role_confidence: 1.0,
                session_id: None,
                sample_rate: sr,
                channels: 1,
                duration_s: 6.0,
                tc_start_s: Some(36000.0),
                offset_s: 0.0,
                offset_confidence: 1.0,
                transcript: None,
                missing: false,
                link_locked: false,
            },
        );
    }
    for (i, (id, ch)) in [("PAUL", "Paul"), ("SARA", "Sara")].iter().enumerate() {
        d.characters.insert(
            id.to_string(),
            Character {
                id: id.to_string(),
                label: ch.to_string(),
                aliases: vec![],
                color_index: i,
            },
        );
    }
    let line = |id: &str, n: u32, ch: &str, text: &str| ScriptLine {
        id: id.into(),
        scene_id: "sc1".into(),
        kind: LineKind::Dialogue,
        character: Some(ch.into()),
        extension: None,
        text: text.into(),
        paren: None,
        tokens: text.split(' ').map(String::from).collect(),
        words: text.split(' ').map(|w| w.to_lowercase()).collect(),
        number: n,
        version_added: 1,
        changed: false,
        forced: false,
        adlib: false,
    };
    d.lines
        .insert("l1".into(), line("l1", 1, "PAUL", "Hello there friend"));
    d.lines
        .insert("l2".into(), line("l2", 2, "SARA", "Hi <Paul> & co"));
    d.takes.insert(
        "t1".into(),
        take(
            "t1",
            "l1",
            "src_p",
            1,
            0.5,
            2.0,
            vec![
                word("Hello", 0.6, 0.9, 0),
                word("there", 1.0, 1.3, 1),
                word("friend", 1.5, 1.9, 2),
            ],
        ),
    );
    d.takes.insert(
        "t1b".into(),
        take(
            "t1b",
            "l1",
            "src_p",
            2,
            3.0,
            4.5,
            vec![
                word("Hello", 3.1, 3.4, 0),
                word("there", 3.5, 3.8, 1),
                word("friend", 4.0, 4.4, 2),
            ],
        ),
    );
    d.takes.insert(
        "t2".into(),
        take(
            "t2",
            "l2",
            "src_s",
            1,
            2.5,
            3.5,
            vec![word("Hi", 2.6, 2.8, 0), word("Paul", 2.9, 3.2, 1)],
        ),
    );
    d.boxes.insert(
        "l1".into(),
        BoxState {
            id: "l1".into(),
            take_id: Some("t1".into()),
            comp: vec![
                CompSegment {
                    take_id: "t1".into(),
                    from_word: 0,
                    to_word: 2,
                    nudge_s: 0.0,
                },
                CompSegment {
                    take_id: "t1b".into(),
                    from_word: 2,
                    to_word: 3,
                    nudge_s: 0.0,
                },
            ],
            ..Default::default()
        },
    );
    d.boxes.insert(
        "l2".into(),
        BoxState {
            id: "l2".into(),
            take_id: Some("t2".into()),
            gap_s: Some(-0.2),
            ..Default::default()
        },
    );
    d.scenes.insert(
        "sc1".into(),
        Scene {
            id: "sc1".into(),
            number: 1,
            heading: "INT. LAB - DAY".into(),
            int_ext: "INT".into(),
            location: "LAB".into(),
            place: "Lab".into(),
            time_of_day: "Day".into(),
            order: vec!["l1".into(), "l2".into()],
            tightness: 1.0,
        },
    );
    d.assets.insert(
        "door".into(),
        Asset {
            id: "door".into(),
            file: "SFX/Misc/door.wav".into(),
            hash: "x".into(),
            category: "SFX/Doors".into(),
            tags: vec![],
            duration_s: 1.0,
            source: AssetSource::Imported,
            license: None,
            prompt: None,
            provider: None,
            lufs: None,
            remote_id: None,
        },
    );
    d.placements.insert(
        "pl".into(),
        Placement {
            id: "pl".into(),
            scene_id: "sc1".into(),
            asset_id: "door".into(),
            cue_id: None,
            lane: Lane::Sfx,
            character: None,
            layer: 0,
            anchor: Some(Anchor {
                line_id: "l2".into(),
                word: None,
                edge: Edge::Start,
            }),
            at_s: 0.0,
            offset_s: -0.5,
            in_s: 0.0,
            len_s: 0.8,
            fade_in_s: 0.0,
            fade_out_s: 0.1,
            fade_curve: Default::default(),
            gain_db: -3.0,
        },
    );
    (paths, d)
}

#[test]
fn fcpxml_is_structurally_valid() {
    let dir = tempfile::tempdir().unwrap();
    let (paths, doc) = setup(dir.path());
    let tl = EpisodeTimeline::build(&doc, &ExportScope::All);
    let xml = fcpxml::write(&doc, &paths, &tl, "Ep & Test");
    let file = dir.path().join("out.fcpxml");
    std::fs::write(&file, &xml).unwrap();
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let x = roxmltree::Document::parse_with_options(&xml, opts).expect("well-formed XML");
    let root = x.root_element();
    assert_eq!(root.tag_name().name(), "fcpxml");
    assert_eq!(root.attribute("version"), Some("1.9"));

    let mut assets: HashMap<&str, (f64, f64)> = HashMap::new();
    for a in x.descendants().filter(|n| n.has_tag_name("asset")) {
        let id = a.attribute("id").unwrap();
        let start = parse_rt(a.attribute("start").unwrap()).unwrap();
        let dur = parse_rt(a.attribute("duration").unwrap()).unwrap();
        let rep = a
            .children()
            .find(|c| c.has_tag_name("media-rep"))
            .expect("media-rep");
        let src = rep.attribute("src").unwrap();
        assert!(src.starts_with("file:///"), "{src}");
        assert!(
            src.contains("Session%201/Paul.wav")
                || src.contains("Sara.wav")
                || src.contains("door.wav")
        );
        assets.insert(id, (start, dur));
    }
    assert_eq!(assets.len(), 3);
    let seq = x
        .descendants()
        .find(|n| n.has_tag_name("sequence"))
        .unwrap();
    let seq_dur = parse_rt(seq.attribute("duration").unwrap()).unwrap();
    let gap = x.descendants().find(|n| n.has_tag_name("gap")).unwrap();

    let mut lanes: HashMap<String, Vec<(f64, f64)>> = HashMap::new();
    let mut seen_marker = false;
    for c in gap.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "asset-clip" => {
                assert!(!seen_marker, "anchored clips come before markers");
                let (astart, adur) = assets[c.attribute("ref").unwrap()];
                let off = parse_rt(c.attribute("offset").unwrap()).unwrap();
                let start = parse_rt(c.attribute("start").unwrap()).unwrap();
                let dur = parse_rt(c.attribute("duration").unwrap()).unwrap();
                assert!(dur > 0.0);
                assert!(
                    start >= astart - 1e-6 && start + dur <= astart + adur + 1e-6,
                    "clip inside its media"
                );
                assert!(off + dur <= seq_dur + 1e-6);
                assert!(c.attribute("lane").unwrap().parse::<i32>().unwrap() < 0);
                assert!(c.attribute("audioRole").is_some());
                for v in c.children().filter(|n| n.has_tag_name("adjust-volume")) {
                    for p in v.children().filter(|n| n.is_element()) {
                        assert_eq!(p.tag_name().name(), "param");
                        for f in p.children().filter(|n| n.is_element()) {
                            assert!(["fadeIn", "fadeOut"].contains(&f.tag_name().name()));
                            assert!(parse_rt(f.attribute("duration").unwrap()).unwrap() > 0.0);
                        }
                    }
                }
                lanes
                    .entry(c.attribute("lane").unwrap().to_string())
                    .or_default()
                    .push((off, off + dur));
            }
            "marker" => {
                seen_marker = true;
                let s = parse_rt(c.attribute("start").unwrap()).unwrap();
                assert!(s >= 0.0 && s <= seq_dur);
            }
            other => panic!("unexpected element in gap: {other}"),
        }
    }
    // one lane per role, plus the effect lane; no overlaps within a lane
    assert_eq!(lanes.len(), 3, "{lanes:?}");
    for spans in lanes.values_mut() {
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        for w in spans.windows(2) {
            assert!(w[0].1 <= w[1].0 + 1e-6, "overlap in lane: {spans:?}");
        }
    }
    // the comp exports as two clips on Paul's lane, with source timecode
    assert_eq!(lanes["-1"].len(), 2);
    assert!(xml.contains("S1.1 PAUL T1 (1/2)") && xml.contains("S1.1 PAUL T2 (2/2)"));
    assert!(
        xml.contains("start=\"1728024000/48000s\""),
        "source timecode 10:00:00 + 0.5 s"
    );
    assert!(
        xml.contains("<project name=\"Ep &amp; Test\">"),
        "names are escaped"
    );
    assert!(xml.contains("value=\"S1 INT. LAB - DAY\""));
    assert!(xml.contains("feedback: liked it"));

    // DTD validation when Final Cut Pro is installed
    let dtd = Path::new("/Applications/Final Cut Pro.app/Contents/Frameworks/Interchange.framework/Versions/A/Resources/FCPXMLv1_9.dtd");
    if dtd.exists()
        && std::process::Command::new("xmllint")
            .arg("--version")
            .output()
            .is_ok()
    {
        let st = std::process::Command::new("xmllint")
            .arg("--noout")
            .arg("--dtdvalid")
            .arg(dtd)
            .arg(&file)
            .status()
            .unwrap();
        assert!(st.success(), "FCPXML fails the 1.9 DTD");
    } else {
        eprintln!("DTD check skipped: Final Cut Pro's FCPXMLv1_9.dtd is not installed; structural checks ran instead");
    }
}

#[test]
fn srt_and_cut_list() {
    let dir = tempfile::tempdir().unwrap();
    let (paths, doc) = setup(dir.path());
    let tl = EpisodeTimeline::build(&doc, &ExportScope::Scenes(vec!["S1".into()]));
    let s = srt::write(&tl, true);
    assert!(s.starts_with("1\n00:00:00,"));
    assert!(s.contains("PAUL: Hello there friend"));
    assert!(s.contains("\n2\n"));
    let rows = cutlist::rows(&doc, &paths, &tl);
    let csv = cutlist::csv(&rows);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "scene,line,character,take,source,source_in,source_out,timeline_in,timeline_out,direction,feedback,text");
    assert_eq!(lines.len(), 1 + 4, "two comp parts, Sara, the door");
    assert!(lines[1].starts_with("S1,S1.1,PAUL,T1 (1/2),Paul.wav,10:00:00.500,"));
    assert!(csv.contains("door.wav"));
    let md = cutlist::markdown("Ep", &rows);
    assert!(md.contains("| S1 | S1.1 | PAUL | T1 (1/2) | Paul.wav |"));
}

#[test]
fn bounce_matches_layout_length() {
    let dir = tempfile::tempdir().unwrap();
    let (paths, doc) = setup(dir.path());
    let (wav, len) = apc_export::bounce::bounce_scene(
        &doc,
        &paths,
        "S1",
        apc_export::bounce::Format::Wav,
        &paths.exports,
    )
    .unwrap();
    let layout = apc_core::timeline::layout_scene(&doc, &doc.scenes["sc1"], &Default::default());
    assert!(
        (len - layout.duration_s).abs() < 0.010,
        "{len} vs {}",
        layout.duration_s
    );
    assert!(wav.exists());
}
