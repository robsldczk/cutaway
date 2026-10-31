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

/// Rule-based condensing used by the mock provider.
pub fn condense_direction(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
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
    if low.contains("present") || low.contains("excited") {
        parts.push("present it");
    }
    if low.contains("slower") {
        parts.push("slower");
    }
    if low.contains("faster") || low.contains("quicker") {
        parts.push("faster");
    }
    if !parts.is_empty() {
        return parts.join(", ");
    }
    few_words(&best_sentence(text), 5)
}

pub fn condense_feedback(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let low = text.to_lowercase();
    match pick_strength(text) {
        2 => return "the pick".into(),
        1 => return "liked it".into(),
        _ => {}
    }
    if low.contains("malfunction") || low.contains("technical") || low.contains("pop") {
        return "technical problem".into();
    }
    if low.contains("too fast") {
        return "too fast".into();
    }
    if low.contains("too slow") {
        return "too slow".into();
    }
    if low.contains("one more") || low.contains("again") {
        return "one more".into();
    }
    few_words(&best_sentence(text), 5)
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
                let notes: Vec<CondensedNote> = input
                    .takes
                    .iter()
                    .map(|t| {
                        let mut tags = tags_of(&t.direction_text);
                        for x in tags_of(&t.feedback_text) {
                            if !tags.contains(&x) {
                                tags.push(x);
                            }
                        }
                        match pick_strength(&t.feedback_text) {
                            2 => tags.push("pick".into()),
                            1 => tags.push("liked".into()),
                            _ => {}
                        }
                        CondensedNote {
                            take_id: t.take_id.clone(),
                            direction: condense_direction(&t.direction_text),
                            feedback: condense_feedback(&t.feedback_text),
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
        };
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
