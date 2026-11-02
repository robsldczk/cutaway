//! As-cut transcript / cut list: scene, line, character, take, source file, source TC in/out,
//! timeline in/out and the notes. CSV and Markdown.

use std::fmt::Write as _;

use apc_core::project::EpisodePaths;
use apc_core::Document;

use crate::fcpxml::media_info;
use crate::timeline::EpisodeTimeline;

/// `HH:MM:SS.mmm`
pub fn tc(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as i64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

pub struct Row {
    pub scene: String,
    pub line: String,
    pub character: String,
    pub take: String,
    pub source: String,
    pub src_in: String,
    pub src_out: String,
    pub rec_in: String,
    pub rec_out: String,
    pub direction: String,
    pub feedback: String,
    pub text: String,
}

pub fn rows(doc: &Document, paths: &EpisodePaths, tl: &EpisodeTimeline) -> Vec<Row> {
    let mut clips: Vec<&crate::timeline::TimelineClip> = tl.clips.iter().collect();
    clips.sort_by(|a, b| a.rec_in.total_cmp(&b.rec_in).then(a.lane.cmp(&b.lane)));
    clips
        .into_iter()
        .map(|c| {
            let m = media_info(doc, paths, &c.source_id);
            let tc0 = m.as_ref().map(|m| m.tc_start_s).unwrap_or(0.0);
            Row {
                scene: format!("S{}", c.scene_number),
                line: c.line_ref.clone(),
                character: c.character.clone().unwrap_or_else(|| c.lane.clone()),
                take: match c.take_no {
                    Some(t) if c.part.1 > 1 => format!("T{t} ({}/{})", c.part.0, c.part.1),
                    Some(t) => format!("T{t}"),
                    None => String::new(),
                },
                source: m
                    .as_ref()
                    .and_then(|m| m.path.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| c.source_id.clone()),
                src_in: tc(tc0 + c.src_in),
                src_out: tc(tc0 + c.src_out),
                rec_in: tc(c.rec_in),
                rec_out: tc(c.rec_out()),
                direction: c.direction.clone(),
                feedback: c.feedback.clone(),
                text: if c.muted {
                    format!("(muted) {}", c.text)
                } else {
                    c.text.clone()
                },
            }
        })
        .collect()
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn csv(rows: &[Row]) -> String {
    let mut out = String::from("scene,line,character,take,source,source_in,source_out,timeline_in,timeline_out,direction,feedback,text\n");
    for r in rows {
        let f = [
            &r.scene,
            &r.line,
            &r.character,
            &r.take,
            &r.source,
            &r.src_in,
            &r.src_out,
            &r.rec_in,
            &r.rec_out,
            &r.direction,
            &r.feedback,
            &r.text,
        ];
        out.push_str(&f.iter().map(|x| csv_field(x)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

pub fn markdown(title: &str, rows: &[Row]) -> String {
    let cell = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let mut out = format!("# As-cut transcript: {title}\n\n| Scene | Line | Character | Take | Source | Source in | Source out | Timeline in | Note | Text |\n|---|---|---|---|---|---|---|---|---|---|\n");
    for r in rows {
        let note = match (r.direction.is_empty(), r.feedback.is_empty()) {
            (true, true) => String::new(),
            (false, true) => r.direction.clone(),
            (true, false) => r.feedback.clone(),
            (false, false) => format!("{} / {}", r.direction, r.feedback),
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            cell(&r.scene),
            cell(&r.line),
            cell(&r.character),
            cell(&r.take),
            cell(&r.source),
            cell(&r.src_in),
            cell(&r.src_out),
            cell(&r.rec_in),
            cell(&note),
            cell(&r.text)
        );
    }
    out
}
