//! FCPXML 1.9 for DaVinci Resolve and Logic Pro (see DECISIONS.md D26).
//!
//! One lane per role (connected clips under an empty gap), clips referencing the original
//! files with their source timecode, fades on every splice, scene and note markers.
//! The assets cover the whole source files, so handles are available in the NLE.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use apc_core::project::EpisodePaths;
use apc_core::Document;

use crate::timeline::EpisodeTimeline;

pub const FCPXML_VERSION: &str = "1.9";
const RATE: f64 = 48000.0;
/// Sequence format: 25 fps (marker duration = one frame).
const FRAME: &str = "100/2500s";

/// Rational time in 1/48000 s units.
pub fn rt(seconds: f64) -> String {
    let n = (seconds * RATE).round() as i64;
    if n == 0 {
        "0s".into()
    } else {
        format!("{n}/48000s")
    }
}

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {}
            c => o.push(c),
        }
    }
    o
}

/// `file:///…` URL with percent-encoding.
pub fn file_url(p: &Path) -> String {
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// What the writer needs to know about a referenced file.
#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub name: String,
    pub path: PathBuf,
    pub duration_s: f64,
    pub tc_start_s: f64,
    pub channels: u16,
    pub sample_rate: u32,
}

/// Resolves source ids (`src_…`, `asset:<id>`) to files.
pub fn media_info(doc: &Document, paths: &EpisodePaths, id: &str) -> Option<MediaInfo> {
    if let Some(aid) = id.strip_prefix("asset:") {
        let a = doc.assets.get(aid)?;
        let path = if Path::new(&a.file).is_absolute() {
            PathBuf::from(&a.file)
        } else {
            paths.library.join(&a.file)
        };
        let probe = apc_media::decode::probe(&path).ok();
        return Some(MediaInfo {
            name: Path::new(&a.file)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| aid.to_string()),
            duration_s: if a.duration_s > 0.0 {
                a.duration_s
            } else {
                probe.map(|p| p.duration_s).unwrap_or(0.0)
            },
            tc_start_s: 0.0,
            channels: probe.map(|p| p.channels).unwrap_or(2),
            sample_rate: probe.map(|p| p.sample_rate).unwrap_or(48000),
            path,
        });
    }
    let s = doc.sources.get(id)?;
    Some(MediaInfo {
        name: Path::new(&s.original_name)
            .file_stem()
            .map(|x| x.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: paths.resolve(&s.path),
        duration_s: s.duration_s,
        tc_start_s: s.tc_start_s.unwrap_or(0.0),
        channels: s.channels.max(1),
        sample_rate: if s.sample_rate > 0 {
            s.sample_rate
        } else {
            48000
        },
    })
}

fn role_of(lane: &str, doc: &Document) -> String {
    if let Some(c) = doc.characters.get(lane) {
        return format!("dialogue.{}", c.label.replace(['.', ' '], "_"));
    }
    if lane.starts_with("SFX") {
        "effects".into()
    } else if lane == "MUSIC" {
        "music".into()
    } else {
        "effects".into()
    }
}

/// Writes the FCPXML document.
pub fn write(
    doc: &Document,
    paths: &EpisodePaths,
    tl: &EpisodeTimeline,
    project_name: &str,
) -> String {
    let mut assets: BTreeMap<String, (String, MediaInfo)> = BTreeMap::new();
    for c in &tl.clips {
        if assets.contains_key(&c.source_id) {
            continue;
        }
        if let Some(info) = media_info(doc, paths, &c.source_id) {
            let rid = format!("r{}", assets.len() + 2);
            assets.insert(c.source_id.clone(), (rid, info));
        }
    }
    let lanes = tl.lanes(doc);
    let lane_no = |l: &str| -(lanes.iter().position(|x| x == l).unwrap_or(lanes.len()) as i64 + 1);
    let total = tl.duration_s.max(0.04);

    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n");
    let _ = writeln!(x, "<fcpxml version=\"{FCPXML_VERSION}\">");
    x.push_str("  <resources>\n");
    let _ = writeln!(x, "    <format id=\"r1\" name=\"FFVideoFormat1080p25\" frameDuration=\"{FRAME}\" width=\"1920\" height=\"1080\"/>");
    for (rid, m) in assets.values() {
        let _ = writeln!(
            x,
            "    <asset id=\"{rid}\" name=\"{}\" start=\"{}\" duration=\"{}\" hasAudio=\"1\" audioSources=\"1\" audioChannels=\"{}\" audioRate=\"{}\">",
            esc(&m.name),
            rt(m.tc_start_s),
            rt(m.duration_s),
            m.channels,
            m.sample_rate
        );
        let _ = writeln!(
            x,
            "      <media-rep kind=\"original-media\" src=\"{}\"/>",
            esc(&file_url(&m.path))
        );
        x.push_str("    </asset>\n");
    }
    x.push_str("  </resources>\n  <library>\n");
    let _ = writeln!(
        x,
        "    <event name=\"{}\">",
        esc(&format!(
            "{} {}",
            apc_core::brand::DISPLAY_NAME,
            project_name
        ))
    );
    let _ = writeln!(x, "      <project name=\"{}\">", esc(project_name));
    let _ = writeln!(
        x,
        "        <sequence format=\"r1\" duration=\"{}\" tcStart=\"0s\" tcFormat=\"NDF\" audioLayout=\"stereo\" audioRate=\"48k\">",
        rt(total)
    );
    x.push_str("          <spine>\n");
    let _ = writeln!(
        x,
        "            <gap name=\"Gap\" offset=\"0s\" start=\"0s\" duration=\"{}\">",
        rt(total)
    );

    // Clips per lane, butt-spliced (no overlap within a lane) with their fades.
    let mut by_lane: BTreeMap<i64, Vec<&crate::timeline::TimelineClip>> = BTreeMap::new();
    for c in &tl.clips {
        by_lane.entry(lane_no(&c.lane)).or_default().push(c);
    }
    let mut ordered: Vec<(f64, String)> = Vec::new();
    for (lane, mut clips) in by_lane {
        clips.sort_by(|a, b| a.rec_in.total_cmp(&b.rec_in));
        for (i, c) in clips.iter().enumerate() {
            let Some((rid, m)) = assets.get(&c.source_id) else {
                continue;
            };
            let mut dur = c.len();
            if let Some(n) = clips.get(i + 1) {
                dur = dur.min((n.rec_in - c.rec_in).max(0.0));
            }
            if dur <= 0.0 {
                continue;
            }
            let name = match (c.character.as_ref(), c.take_no) {
                (Some(ch), Some(t)) if c.part.1 > 1 => {
                    format!("{} {} T{} ({}/{})", c.line_ref, ch, t, c.part.0, c.part.1)
                }
                (Some(ch), Some(t)) => format!("{} {} T{}", c.line_ref, ch, t),
                _ => format!("{} {}", c.line_ref, c.text),
            };
            let mut s = String::new();
            let _ = write!(
                s,
                "              <asset-clip ref=\"{rid}\" lane=\"{lane}\" offset=\"{}\" name=\"{}\" start=\"{}\" duration=\"{}\" audioRole=\"{}\" tcFormat=\"NDF\">",
                rt(c.rec_in),
                esc(&name),
                rt(m.tc_start_s + c.src_in),
                rt(dur),
                esc(&role_of(&c.lane, doc))
            );
            let fade_in = c.fade_in_s.min(dur / 2.0);
            let fade_out = c.fade_out_s.min(dur / 2.0);
            if c.gain_db.abs() > 0.01 || fade_in > 0.0 || fade_out > 0.0 {
                let _ = write!(
                    s,
                    "\n                <adjust-volume amount=\"{:+.1}dB\">",
                    c.gain_db
                );
                if fade_in > 0.0 || fade_out > 0.0 {
                    s.push_str("\n                  <param name=\"amount\">");
                    if fade_in > 0.0 {
                        let _ = write!(
                            s,
                            "\n                    <fadeIn type=\"easeIn\" duration=\"{}\"/>",
                            rt(fade_in)
                        );
                    }
                    if fade_out > 0.0 {
                        let _ = write!(
                            s,
                            "\n                    <fadeOut type=\"easeOut\" duration=\"{}\"/>",
                            rt(fade_out)
                        );
                    }
                    s.push_str("\n                  </param>");
                }
                s.push_str("\n                </adjust-volume>\n              ");
            }
            s.push_str("</asset-clip>\n");
            ordered.push((c.rec_in, s));
        }
    }
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, s) in ordered {
        x.push_str(&s);
    }

    // Markers: scenes, then notes.
    let mut markers: Vec<(f64, String)> = Vec::new();
    for sc in &tl.scenes {
        markers.push((sc.start_s, format!("S{} {}", sc.number, sc.heading)));
    }
    for c in tl
        .clips
        .iter()
        .filter(|c| c.part.0 == 1 && (!c.direction.is_empty() || !c.feedback.is_empty()))
    {
        let who = c.character.clone().unwrap_or_default();
        let mut v = format!("{} {} T{}", c.line_ref, who, c.take_no.unwrap_or(0));
        if !c.direction.is_empty() {
            let _ = write!(v, " · direction: {}", c.direction);
        }
        if !c.feedback.is_empty() {
            let _ = write!(v, " · feedback: {}", c.feedback);
        }
        markers.push((c.rec_in, v));
    }
    markers.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (t, v) in markers {
        let _ = writeln!(
            x,
            "              <marker start=\"{}\" duration=\"{FRAME}\" value=\"{}\"/>",
            rt(t),
            esc(&v)
        );
    }
    x.push_str("            </gap>\n          </spine>\n        </sequence>\n      </project>\n    </event>\n  </library>\n</fcpxml>\n");
    x
}

/// Parses a rational FCPXML time (`"123/48000s"`, `"5s"`, `"0s"`).
pub fn parse_rt(s: &str) -> Option<f64> {
    let s = s.strip_suffix('s')?;
    match s.split_once('/') {
        Some((n, d)) => Some(n.parse::<f64>().ok()? / d.parse::<f64>().ok()?),
        None => s.parse().ok(),
    }
}
