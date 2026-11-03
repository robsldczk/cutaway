//! Mock AI provider for tests and offline use: deterministic rules per task, or recorded
//! responses (`<dir>/<task>/<input-hash>.json`) when a recordings folder is set.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::provider::{AiError, AiProvider, AiRequest};
use crate::task::*;

/// Explicit picks (Stage 0 `STRONG`).
pub const STRONG_PICK: &[&str] = &[
    "that was the one",
    "the one i liked",
    "right one",
    "we have the",
    "perfect",
    "definitely",
];
/// Liked it (Stage 0 `WEAK`).
pub const WEAK_PICK: &[&str] = &[
    "very good",
    "i like that",
    "i liked",
    "i like the",
    "nice",
    "awesome",
    "good",
];

/// Pick strength from feedback text: 2 explicit, 1 liked, 0 none.
pub fn pick_strength(feedback: &str) -> u8 {
    let low = feedback.to_lowercase();
    if STRONG_PICK.iter().any(|k| low.contains(k)) {
        2
    } else if WEAK_PICK.iter().any(|k| low.contains(k)) {
        1
    } else {
        0
    }
}

const TAGS: &[(&str, &[&str])] = &[
    ("quieter", &["quieter", "to yourself", "softer"]),
    (
        "energy",
        &["oomph", "spice", "crazy", "excitement", "energ", "louder"],
    ),
    ("emphasis", &["emphasi"]),
    ("annoyed", &["annoyed", "annoying"]),
    ("excited", &["excited", "proud", "present"]),
    (
        "re-take",
        &[
            "one more",
            "again",
            "a couple times",
            "from the top",
            "same thing",
        ],
    ),
    (
        "technical",
        &["pop", "malfunction", "hand in front", "couldn't hear"],
    ),
    ("slower", &["slower", "take your time"]),
    ("faster", &["faster", "quicker", "pick up the pace"]),
];

fn tags_of(text: &str) -> Vec<String> {
    let low = text.to_lowercase();
    TAGS.iter()
        .filter(|(_, kws)| kws.iter().any(|k| low.contains(k)))
        .map(|(t, _)| t.to_string())
        .collect()
}

fn few_words(text: &str, n: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut s = words[..words.len().min(n)].join(" ");
    s = s.trim_end_matches([',', '.', '?', '!', ';']).to_string();
    if words.len() > n {
        s.push('…');
    }
    s
}

/// The sentence with the most instruction keywords (Stage 0 `summarize`).
fn best_sentence(text: &str) -> String {
    let key = [
        "give", "more", "like", "emphas", "quieter", "annoy", "excit", "one", "present", "not",
        "pop", "try",
    ];
    let mut best = String::new();
    let mut score = -1i32;
    for s in text.split_inclusive(['.', '?', '!']) {
        let s = s.trim();
        if s.split_whitespace().count() < 3 {
            continue;
        }
        let low = s.to_lowercase();
        let k = key.iter().filter(|k| low.contains(*k)).count() as i32;
        if k > score {
            score = k;
            best = s.to_string();
        }
    }
    if best.is_empty() {
        text.trim().to_string()
    } else {
        best
    }
}

/// Instruction keywords in room talk ("quieter", "to yourself").
fn direction_keywords(text: &str) -> Vec<&'static str> {
    let low = text.to_lowercase();
    let mut parts: Vec<&str> = Vec::new();
    if low.contains("quieter") || low.contains("softer") {
        parts.push("quieter");
    }
    if low.contains("to yourself") {
        parts.push("to yourself");
    }
    if ["oomph", "spice", "crazy", "excitement", "louder"]
        .iter()
        .any(|k| low.contains(k))
    {
        parts.push("more energy");
    }
    if low.contains("emphasi") {
        parts.push("more emphasis");
    }
    if low.contains("annoyed") {
        parts.push("annoyed");
    }
    if low.contains("curious") {
        parts.push("curious");
    }
    if low.contains("present") || low.contains("excited") {
        parts.push("present it");
    }
    if low.contains("slower") {
        parts.push("slower");
    }
    if low.contains("faster") || low.contains("quicker") {
        parts.push("faster");
    }
    parts
}

/// A reaction keyword in room talk ("liked it", "technical problem").
fn feedback_keyword(text: &str) -> Option<&'static str> {
    let low = text.to_lowercase();
    match pick_strength(text) {
        2 => return Some("the pick"),
        1 => return Some("liked it"),
        _ => {}
    }
    if low.contains("malfunction") || low.contains("technical") || low.contains("pop") {
        return Some("technical problem");
    }
    if low.contains("too fast") {
        return Some("too fast");
    }
    if low.contains("too slow") {
        return Some("too slow");
    }
    if low.contains("one more") || low.contains("again") {
        return Some("one more");
    }
    None
}

/// Rule-based condensing used by the mock provider.
pub fn condense_direction(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let parts = direction_keywords(text);
    if !parts.is_empty() {
        return parts.join(", ");
    }
    few_words(&best_sentence(text), 5)
}

pub fn condense_feedback(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    match feedback_keyword(text) {
        Some(k) => k.into(),
        None => few_words(&best_sentence(text), 5),
    }
}

/// Rule-based note on the room talk between two takes: the reaction to the previous
/// take and the setup for the next. Claude interprets the talk; this is the mock and
/// the fallback when it can't.
pub fn condense_gap(text: &str, has_previous: bool, has_next: bool) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let fb = feedback_keyword(text).filter(|_| has_previous);
    let dir = Some(direction_keywords(text))
        .filter(|d| has_next && !d.is_empty())
        .map(|d| d.join(", "));
    let mut out: Vec<String> = Vec::new();
    if let Some(fb) = fb {
        out.push(format!("After the take: {fb}."));
    }
    if let Some(dir) = dir {
        out.push(format!("For the next: {dir}."));
    }
    if out.is_empty() {
        out.push(format!(
            "Room: \u{201c}{}\u{201d}",
            few_words(&best_sentence(text), 8)
        ));
    }
    out.join(" ")
}

const SPOT_RULES: &[(&[&str], &str, SpotCueType, u8)] = &[
    (&["door"], "door", SpotCueType::Hard, 1),
    (&["knock"], "knock on a door", SpotCueType::Foley, 1),
    (
        &["phone rings", "ringing phone", "rings inside"],
        "phone ringing",
        SpotCueType::Hard,
        1,
    ),
    (&["dials"], "phone dialing", SpotCueType::Hard, 2),
    (&["hangs up"], "phone hung up", SpotCueType::Foley, 2),
    (&["alarm"], "alarm", SpotCueType::Hard, 1),
    (&["dings", "bell"], "bell ding", SpotCueType::Hard, 1),
    (
        &["scribble"],
        "pen scribbling on a blackboard",
        SpotCueType::Foley,
        2,
    ),
    (
        &["types", "typing"],
        "keyboard typing",
        SpotCueType::Foley,
        2,
    ),
    (
        &["paces", "walks", "runs", "sprints", "rushes"],
        "footsteps",
        SpotCueType::Foley,
        2,
    ),
    (
        &["sets down", "plops"],
        "object set down on a table",
        SpotCueType::Foley,
        2,
    ),
    (&["coffee"], "coffee machine", SpotCueType::Hard, 3),
    (
        &["knife", "sharpen"],
        "knife sharpening",
        SpotCueType::Foley,
        2,
    ),
    (&["button"], "big button press", SpotCueType::Hard, 1),
    (&["lever"], "lever pull", SpotCueType::Hard, 1),
    (
        &["ufo", "tractor beam"],
        "sci-fi ship hum",
        SpotCueType::Hard,
        1,
    ),
    (
        &["belches", "glows", "activates"],
        "machine power-up swell",
        SpotCueType::Hard,
        1,
    ),
    (
        &["throws"],
        "notebook thrown against a wall",
        SpotCueType::Foley,
        2,
    ),
    (
        &["blackboard", "rips down"],
        "blackboard rolling down",
        SpotCueType::Hard,
        2,
    ),
    (
        &["compiles", "computer"],
        "computer beeps",
        SpotCueType::Hard,
        3,
    ),
    (&["cage"], "cage door squeak", SpotCueType::Foley, 2),
    (
        &["pages", "flipping"],
        "pages flipping",
        SpotCueType::Foley,
        3,
    ),
    (&["steaming"], "sizzle and steam", SpotCueType::Foley, 3),
    (
        &["wrapped", "aluminum"],
        "aluminum foil rustle",
        SpotCueType::Foley,
        2,
    ),
];

fn spot(input: &SoundSpotInput) -> SoundSpotOutput {
    let mut cues = Vec::new();
    if let Some(first) = input.scene.items.first() {
        cues.push(SpottedCue {
            description: format!("{} ambience", input.scene.location.to_lowercase()),
            anchor_line_id: first.line_id.clone(),
            anchor_word: None,
            cue_type: SpotCueType::Ambience,
            tags: vec!["ambience".into(), "room tone".into()],
            priority: 1,
        });
    }
    for item in &input.scene.items {
        if item.kind != "action" {
            continue;
        }
        let low = item.text.to_lowercase();
        let words: Vec<String> = low
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .collect();
        for (kws, desc, ty, prio) in SPOT_RULES {
            let Some(k) = kws.iter().find(|k| low.contains(*k)) else {
                continue;
            };
            let first = k.split_whitespace().next().unwrap_or(k);
            let word = words.iter().position(|w| w.starts_with(first));
            cues.push(SpottedCue {
                description: desc.to_string(),
                anchor_line_id: item.line_id.clone(),
                anchor_word: word,
                cue_type: *ty,
                tags: desc
                    .split_whitespace()
                    .filter(|w| w.len() > 3)
                    .map(String::from)
                    .collect(),
                priority: *prio,
            });
        }
    }
    SoundSpotOutput { cues }
}

fn sound_tag(input: &SoundTagInput) -> SoundTagOutput {
    let low = format!(
        "{} {}",
        input.file_name,
        input.cue_description.clone().unwrap_or_default()
    )
    .to_lowercase();
    let cat = [
        ("door", "SFX/Doors"),
        ("knock", "SFX/Doors"),
        ("alarm", "SFX/Alarms"),
        ("siren", "SFX/Alarms"),
        ("phone", "SFX/Phones"),
        ("ring", "SFX/Phones"),
        ("hum", "SFX/Machines"),
        ("machine", "SFX/Machines"),
        ("engine", "SFX/Vehicles"),
        ("car", "SFX/Vehicles"),
        ("step", "SFX/Foley"),
        ("foley", "SFX/Foley"),
        ("cloth", "SFX/Foley"),
        ("hit", "SFX/Impacts"),
        ("impact", "SFX/Impacts"),
        ("crash", "SFX/Impacts"),
        ("turkey", "SFX/Animals"),
        ("bird", "SFX/Animals"),
        ("dog", "SFX/Animals"),
        ("amb", "Ambience"),
        ("room tone", "Ambience"),
        ("roomtone", "Ambience"),
        ("music", "Music"),
        ("theme", "Music"),
    ]
    .iter()
    .find(|(k, _)| low.contains(k))
    .map(|(_, c)| c.to_string())
    .unwrap_or_else(|| {
        if input.duration_s > 20.0 {
            "Ambience".into()
        } else {
            "SFX/Misc".into()
        }
    });
    let stem = input
        .file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(&input.file_name);
    let tags = stem
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 2 && !t.chars().all(|c| c.is_ascii_digit()))
        .map(|t| t.to_lowercase())
        .collect();
    SoundTagOutput {
        category: cat,
        tags,
    }
}

fn name_match(input: &NameMatchInput) -> NameMatchOutput {
    let room = ["room", "director", "regie", "talkback", "tb"];
    let assignments = input
        .files
        .iter()
        .map(|f| {
            let toks: Vec<String> = f
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .map(String::from)
                .collect();
            if toks.iter().any(|t| room.contains(&t.as_str())) {
                return NameAssignment {
                    file: f.clone(),
                    role: NameRole::Room,
                    character: None,
                    confidence: 0.8,
                };
            }
            let hit = input.characters.iter().find(|c| {
                c.to_lowercase()
                    .split_whitespace()
                    .any(|w| w.len() > 2 && toks.iter().any(|t| t == w))
            });
            let alias = input
                .aliases
                .iter()
                .find(|(a, _)| toks.iter().any(|t| *t == a.to_lowercase()));
            match (hit, alias) {
                (Some(c), _) => NameAssignment {
                    file: f.clone(),
                    role: NameRole::Character,
                    character: Some(c.clone()),
                    confidence: 0.7,
                },
                (None, Some((_, c))) => NameAssignment {
                    file: f.clone(),
                    role: NameRole::Character,
                    character: Some(c.clone()),
                    confidence: 0.7,
                },
                _ => NameAssignment {
                    file: f.clone(),
                    role: NameRole::Unknown,
                    character: None,
                    confidence: 0.2,
                },
            }
        })
        .collect();
    NameMatchOutput { assignments }
}

fn word_overlap(a: &str, b: &str) -> f64 {
    let norm = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|w| !w.is_empty())
            .map(String::from)
            .collect()
    };
    let (wa, wb) = (norm(a), norm(b));
    if wb.is_empty() {
        return 0.0;
    }
    let hits = wb.iter().filter(|w| wa.contains(w)).count();
    hits as f64 / wb.len() as f64
}

fn line_match(input: &LineMatchInput) -> LineMatchOutput {
    let matches = input
        .segments
        .iter()
        .map(|s| {
            // Rules can't judge sound-alikes ("Mm hmm" for "Mh-hm"): keep the aligner's proposal.
            if let Some(p) = &s.proposed_line_id {
                return LineMatch {
                    index: s.index,
                    line_id: Some(p.clone()),
                    take_type: if s.score >= 85.0 {
                        MatchTakeType::Full
                    } else {
                        MatchTakeType::Partial
                    },
                    confidence: (s.score / 100.0).clamp(0.0, 1.0),
                };
            }
            let best = input
                .candidates
                .iter()
                .map(|c| (c, word_overlap(&s.text, &c.text)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            match best {
                Some((c, ov)) if ov >= 0.5 => LineMatch {
                    index: s.index,
                    line_id: Some(c.line_id.clone()),
                    take_type: if ov >= 0.85 {
                        MatchTakeType::Full
                    } else {
                        MatchTakeType::Partial
                    },
                    confidence: ov.min(1.0),
                },
                _ => LineMatch {
                    index: s.index,
                    line_id: None,
                    take_type: MatchTakeType::Adlib,
                    confidence: 0.5,
                },
            }
        })
        .collect();
    LineMatchOutput { matches }
}

/// Answers every task with deterministic rules (or recorded responses).
pub struct MockProvider {
    pub recordings: Option<PathBuf>,
}

impl MockProvider {
    pub fn new() -> Self {
        MockProvider { recordings: None }
    }

    pub fn with_recordings(dir: PathBuf) -> Self {
        MockProvider {
            recordings: Some(dir),
        }
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Word-overlap similarity (Dice on normalised words), 0..1.
fn overlap(a: &str, b: &str) -> f64 {
    let a = apc_core::text::norm_words(a);
    let b = apc_core::text::norm_words(b);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut rest = b.clone();
    let mut common = 0;
    for w in &a {
        if let Some(i) = rest.iter().position(|x| x == w) {
            rest.swap_remove(i);
            common += 1;
        }
    }
    2.0 * common as f64 / (a.len() + b.len()) as f64
}

/// Rules version of the rescue: a left-over sentence that shares most of its words with
/// one of the actor's lines is a take of it; a sentence the editor linked to a line is.
fn take_rescue(input: &TakeRescueInput) -> TakeRescueOutput {
    let own: Vec<&RescueScriptLine> = input
        .script
        .iter()
        .filter(|l| l.character == input.character)
        .collect();
    let mut rescues = Vec::new();
    let mut done = std::collections::HashSet::new();
    for h in &input.hints {
        for id in &h.ids {
            let Some(row) = input.recording.iter().find(|r| &r.id == id) else {
                continue;
            };
            if !row.id.starts_with('s') || !done.insert(id.clone()) {
                continue;
            }
            let line = h.line.clone().filter(|l| own.iter().any(|o| &o.label == l));
            let read_as = line
                .as_ref()
                .and_then(|l| own.iter().find(|o| &o.label == l))
                .map(|o| o.text.clone())
                .unwrap_or_default();
            rescues.push(Rescue {
                id: id.clone(),
                confidence: if line.is_some() {
                    RescueConfidence::High
                } else {
                    RescueConfidence::Low
                },
                line,
                read_as,
                from: None,
                to: None,
            });
        }
    }
    for row in input.recording.iter().filter(|r| r.tag == "?") {
        if done.contains(&row.id) {
            continue;
        }
        let best = own
            .iter()
            .map(|l| (overlap(&row.text, &l.text), *l))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((sim, l)) = best {
            if sim >= 0.5 {
                rescues.push(Rescue {
                    id: row.id.clone(),
                    line: Some(l.label.clone()),
                    read_as: l.text.clone(),
                    confidence: if sim >= 0.7 {
                        RescueConfidence::High
                    } else {
                        RescueConfidence::Medium
                    },
                    from: None,
                    to: None,
                });
            }
        }
    }
    TakeRescueOutput { rescues }
}

fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, AiError> {
    serde_json::from_value(v.clone())
        .map_err(|e| AiError::Failed(format!("mock got bad input: {e}")))
}

impl AiProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "rules-v1"
    }

    fn run(&self, req: &AiRequest) -> Result<Value, AiError> {
        if let Some(dir) = &self.recordings {
            let key = blake3::hash(serde_json::to_string(&req.input)?.as_bytes())
                .to_hex()
                .to_string();
            let p = dir.join(req.task.as_str()).join(format!("{key}.json"));
            if let Ok(s) = std::fs::read_to_string(&p) {
                return Ok(serde_json::from_str(&s)?);
            }
        }
        let v = match req.task {
            AiTask::LineMatch => serde_json::to_value(line_match(&parse(&req.input)?))?,
            AiTask::RoomClassify => {
                let input: RoomClassifyInput = parse(&req.input)?;
                json!({ "segments": input.segments.iter().map(|s| json!({"id": s.id, "class": s.hint})).collect::<Vec<_>>() })
            }
            AiTask::NoteCondense => {
                let input: NoteCondenseInput = parse(&req.input)?;
                let notes: Vec<GapNote> = input
                    .gaps
                    .iter()
                    .map(|g| {
                        let mut tags = tags_of(&g.text);
                        if g.previous.is_some() {
                            match pick_strength(&g.text) {
                                2 => tags.push("pick".into()),
                                1 => tags.push("liked".into()),
                                _ => {}
                            }
                        }
                        GapNote {
                            gap_id: g.gap_id.clone(),
                            summary: condense_gap(&g.text, g.previous.is_some(), g.next.is_some()),
                            tags,
                        }
                    })
                    .collect();
                serde_json::to_value(NoteCondenseOutput { notes })?
            }
            AiTask::PickDetect => {
                let input: PickDetectInput = parse(&req.input)?;
                let picks: Vec<Pick> = input
                    .takes
                    .iter()
                    .map(|t| Pick {
                        take_id: t.take_id.clone(),
                        strength: pick_strength(&t.feedback_text),
                        refers_to: Some(t.take_id.clone()),
                    })
                    .collect();
                serde_json::to_value(PickDetectOutput { picks })?
            }
            AiTask::SoundSpot => serde_json::to_value(spot(&parse(&req.input)?))?,
            AiTask::SoundTag => serde_json::to_value(sound_tag(&parse(&req.input)?))?,
            AiTask::NameMatch => serde_json::to_value(name_match(&parse(&req.input)?))?,
            AiTask::TakeRescue => serde_json::to_value(take_rescue(&parse(&req.input)?))?,
        };
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rescue_rules_take_own_lines_only() {
        let row = |id: &str, speaker: &str, text: &str, tag: &str| RescueRow {
            id: id.into(),
            speaker: speaker.into(),
            text: text.into(),
            tag: tag.into(),
            pause_before_s: None,
        };
        let input = TakeRescueInput {
            character: "SARA".into(),
            scene: "4".into(),
            script: vec![
                RescueScriptLine {
                    label: "L1".into(),
                    character: "SARA".into(),
                    text: "It's a little bit late to get a Turkey, though…".into(),
                },
                RescueScriptLine {
                    label: "L2".into(),
                    character: "PAUL".into(),
                    text: "Germany doesn't have Thanksgiving?".into(),
                },
            ],
            recording: vec![
                row(
                    "s1",
                    "SARA",
                    "it's a lid of a bit late to get a tricky though",
                    "?",
                ),
                row("s2", "SARA", "Germany doesn't have Thanksgiving?", "?"),
                row(
                    "d3",
                    "DIRECTOR",
                    "it's a little bit late to get a turkey",
                    "",
                ),
                row("s4", "SARA", "can I get some water", "?"),
            ],
            hints: vec![],
            examples: vec![],
        };
        let out = take_rescue(&input);
        let got: Vec<(&str, Option<&str>)> = out
            .rescues
            .iter()
            .map(|r| (r.id.as_str(), r.line.as_deref()))
            .collect();
        assert_eq!(
            got,
            [("s1", Some("L1"))],
            "Paul's line and the director never"
        );
        let text = input.text().unwrap();
        assert!(text.contains("s1 SARA [?]: it's a lid"), "{text}");
        assert!(text.contains("d3 DIRECTOR: it's a little"), "{text}");
        assert!(!text.contains("\"t0\""), "no timecodes in the text");
    }

    #[test]
    fn picks_and_notes_from_stage0_phrases() {
        assert_eq!(pick_strength("I think that that one was the right one."), 2);
        assert_eq!(pick_strength("Okay, we have the edible, definitely."), 2);
        assert_eq!(pick_strength("I like that."), 1);
        assert_eq!(pick_strength("One more time, just annoyed."), 0);
        assert_eq!(
            condense_direction("Can you give me one more uh a little quieter to yourself?"),
            "quieter, to yourself"
        );
        assert_eq!(condense_feedback("I like that."), "liked it");
        assert_eq!(
            condense_feedback("We had a technical malfunction here, but"),
            "technical problem"
        );
    }

    #[test]
    fn gap_notes_read_both_ways() {
        let t = "I like that. Can you give me one more a little quieter to yourself?";
        assert_eq!(
            condense_gap(t, true, true),
            "After the take: liked it. For the next: quieter, to yourself."
        );
        assert_eq!(
            condense_gap(t, false, true),
            "For the next: quieter, to yourself."
        );
        assert_eq!(condense_gap(t, true, false), "After the take: liked it.");
        assert_eq!(
            condense_gap("That was the one I liked.", true, true),
            "After the take: the pick."
        );
        assert_eq!(
            condense_gap("Okay, let's start with the lab scene.", false, true),
            "Room: \u{201c}Okay, let's start with the lab scene\u{201d}"
        );
        assert_eq!(condense_gap("  ", true, true), "");
    }

    #[test]
    fn mock_answers_every_gap() {
        let input = NoteCondenseInput {
            gaps: vec![
                RoomGap {
                    gap_id: "g1".into(),
                    previous: None,
                    next: Some(GapTake {
                        take_id: "t1".into(),
                        character: "PAUL".into(),
                        line_text: "Lunch.".into(),
                    }),
                    text: "Okay, a little quieter, to yourself.".into(),
                },
                RoomGap {
                    gap_id: "g2".into(),
                    previous: Some(GapTake {
                        take_id: "t1".into(),
                        character: "PAUL".into(),
                        line_text: "Lunch.".into(),
                    }),
                    next: None,
                    text: "I think that that one was the right one.".into(),
                },
            ],
        };
        let req = AiRequest {
            task: AiTask::NoteCondense,
            prompt: String::new(),
            prompt_version: 2,
            input: serde_json::to_value(&input).unwrap(),
            text: None,
            output_schema: Value::Null,
        };
        let out: NoteCondenseOutput =
            serde_json::from_value(MockProvider::new().run(&req).unwrap()).unwrap();
        assert_eq!(out.notes.len(), 2);
        assert_eq!(out.notes[0].summary, "For the next: quieter, to yourself.");
        assert!(out.notes[1].tags.contains(&"pick".to_string()));
    }

    #[test]
    fn spotting_finds_the_burrito() {
        let input = SoundSpotInput {
            scene: SpotScene {
                id: "s2".into(),
                heading: "INT. TARMAC - CANTEEN - LATER".into(),
                location: "TARMAC - CANTEEN".into(),
                items: vec![ScriptItem {
                    line_id: "l1".into(),
                    kind: "action".into(),
                    character: None,
                    text: "Paul sets down a STEAMING, aluminum-wrapped BURRITO.".into(),
                }],
            },
            context: None,
        };
        let out = spot(&input);
        assert_eq!(out.cues[0].cue_type, SpotCueType::Ambience);
        assert!(out
            .cues
            .iter()
            .any(|c| c.description.contains("set down") && c.anchor_word == Some(1)));
        assert!(out.validate().is_ok());
    }
}
