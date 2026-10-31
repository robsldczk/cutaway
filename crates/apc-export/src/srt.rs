//! SRT subtitles from the final word timings.

use std::fmt::Write as _;

use crate::timeline::EpisodeTimeline;

fn ts(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as i64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

pub fn write(tl: &EpisodeTimeline, with_names: bool) -> String {
    let mut out = String::new();
    let mut cues = tl.cues.clone();
    cues.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    for (i, c) in cues.iter().enumerate() {
        let text = match (&c.character, with_names) {
            (Some(ch), true) => format!("{ch}: {}", c.text),
            _ => c.text.clone(),
        };
        let _ = write!(
            out,
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            ts(c.start_s),
            ts(c.end_s),
            text
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps() {
        assert_eq!(ts(3723.0456), "01:02:03,046");
        assert_eq!(ts(0.0), "00:00:00,000");
    }
}
