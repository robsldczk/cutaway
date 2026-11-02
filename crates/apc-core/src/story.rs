//! Story context from the writers' tool (Throughline): character bios and settings.
//! Kept per episode and read-only in the app; the newest import wins field by field:
//! its non-empty fields replace the stored ones, empty fields don't erase (D54).

use serde::{Deserialize, Serialize};

use crate::bus::Tx;
use crate::error::Result;
use crate::model::{CharacterProfile, Document, Location, SceneStory};
use crate::script::location_id;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct StoryContext {
    pub characters: Vec<StoryCharacter>,
    pub locations: Vec<StoryLocation>,
}

impl StoryContext {
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty() && self.locations.is_empty()
    }
}

/// A character as the writers named it ("The Colonel"), with its bio.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct StoryCharacter {
    pub name: String,
    #[serde(default)]
    pub profile: CharacterProfile,
}

/// A setting by its heading name ("TARMAC - CANTEEN"), with the setting it belongs to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct StoryLocation {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Name of the parent setting.
    #[serde(default)]
    pub parent: Option<String>,
}

/// What an import changed in the story context (part of the change summary).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
#[serde(default)]
pub struct StorySummary {
    pub characters_updated: Vec<String>,
    pub locations_updated: Vec<String>,
    /// Names with a bio that match no character of the script (add an alias, re-import).
    pub unknown_characters: Vec<String>,
}

impl StorySummary {
    pub fn is_empty(&self) -> bool {
        self == &StorySummary::default()
    }
}

/// A setting name as a heading writes it: dash variants become " - ".
pub fn location_name(s: &str) -> String {
    s.replace(" \u{2013} ", " - ")
        .replace(" \u{2014} ", " - ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn norm_name(s: &str) -> String {
    s.replace(['\u{2019}', '\u{2018}'], "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn bare(s: &str) -> &str {
    s.strip_prefix("THE ").unwrap_or(s)
}

/// The script character a writer's name refers to: same name, same without a leading
/// "THE" (COLONEL ↔ THE COLONEL), or one of the character's aliases.
pub fn character_for(doc: &Document, name: &str) -> Option<String> {
    let key = norm_name(name);
    if !key.chars().any(char::is_alphabetic) {
        return None;
    }
    let chars = || doc.characters.values();
    chars()
        .find(|c| norm_name(&c.id) == key)
        .or_else(|| chars().find(|c| bare(&norm_name(&c.id)) == bare(&key)))
        .or_else(|| chars().find(|c| c.aliases.iter().any(|a| norm_name(a) == key)))
        .map(|c| c.id.clone())
}

fn newer(old: &str, new: &str) -> String {
    if new.trim().is_empty() {
        old.to_string()
    } else {
        new.trim().to_string()
    }
}

fn merge_profile(old: &CharacterProfile, new: &CharacterProfile) -> CharacterProfile {
    CharacterProfile {
        summary: newer(&old.summary, &new.summary),
        want: newer(&old.want, &new.want),
        need: newer(&old.need, &new.need),
        flaw: newer(&old.flaw, &new.flaw),
        notes: newer(&old.notes, &new.notes),
    }
}

/// A scene's story after a new version: the version's non-empty fields win.
pub fn merge_scene(old: Option<&SceneStory>, new: &SceneStory) -> SceneStory {
    let Some(old) = old else {
        return new.clone();
    };
    SceneStory {
        title: newer(&old.title, &new.title),
        synopsis: newer(&old.synopsis, &new.synopsis),
        act: newer(&old.act, &new.act),
        cast: if new.cast.is_empty() {
            old.cast.clone()
        } else {
            new.cast.clone()
        },
        tone: new.tone.or(old.tone),
        beat_id: new.beat_id.clone().or_else(|| old.beat_id.clone()),
    }
}

/// Merges imported story context into the document (inside a command).
pub fn merge(tx: &mut Tx, story: &StoryContext) -> Result<StorySummary> {
    let mut summary = StorySummary::default();
    for sc in &story.characters {
        let Some(id) = character_for(tx.doc(), &sc.name) else {
            if !sc.profile.is_empty() && sc.name.chars().any(char::is_alphabetic) {
                summary.unknown_characters.push(sc.name.clone());
            }
            continue;
        };
        let Some(c) = tx.doc().characters.get(&id) else {
            continue;
        };
        let profile = merge_profile(&c.profile, &sc.profile);
        if profile != c.profile {
            let mut c = c.clone();
            c.profile = profile;
            tx.put(&c)?;
            if !summary.characters_updated.contains(&id) {
                summary.characters_updated.push(id);
            }
        }
    }

    // A setting nobody plays in is only worth keeping as the parent of one that is.
    let parents: Vec<String> = story
        .locations
        .iter()
        .filter_map(|l| l.parent.as_deref().map(|p| location_id(&location_name(p))))
        .collect();
    let mut all = story.locations.clone();
    for l in &story.locations {
        if let Some(p) = &l.parent {
            let pid = location_id(&location_name(p));
            if !all
                .iter()
                .any(|x| location_id(&location_name(&x.name)) == pid)
            {
                all.push(StoryLocation {
                    name: p.clone(),
                    description: String::new(),
                    parent: None,
                });
            }
        }
    }
    for sl in &all {
        let name = location_name(&sl.name);
        let id = location_id(&name);
        if name.is_empty() || id == "loc_" {
            continue;
        }
        let parent = sl
            .parent
            .as_deref()
            .map(|p| location_id(&location_name(p)))
            .filter(|p| p != &id);
        let old = match tx.doc().locations.get(&id) {
            Some(l) => l.clone(),
            None if !sl.description.trim().is_empty() || parents.contains(&id) => Location {
                id: id.clone(),
                name: name.clone(),
                room_tone: Default::default(),
                description: String::new(),
                parent: None,
            },
            None => continue,
        };
        let mut l = old.clone();
        l.description = newer(&old.description, &sl.description);
        if parent.is_some() {
            l.parent = parent;
        }
        if tx.doc().locations.get(&id) != Some(&l) {
            tx.put(&l)?;
            summary.locations_updated.push(name);
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::CommandBus;
    use crate::commands::Command;
    use crate::fountain::{self, ParsedScript};
    use crate::model::{RoomTone, ScriptVersion};
    use crate::script::ChangeSummary;
    use crate::store::EpisodeStore;

    fn bus() -> (tempfile::TempDir, CommandBus) {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::create(&dir.path().join("t.cutaway")).unwrap();
        (dir, CommandBus::new(store, "test").unwrap())
    }

    fn import(bus: &mut CommandBus, n: u32, parsed: ParsedScript) -> ChangeSummary {
        let version = ScriptVersion {
            id: format!("sv_{n}"),
            number: n,
            file: format!("Script/v{n:03} test.thrl"),
            name: "test".into(),
            hash: String::new(),
            imported_at: String::new(),
            summary: serde_json::Value::Null,
            title: Default::default(),
        };
        let r = bus
            .execute(&Command::ImportScriptVersion { version, parsed })
            .unwrap();
        serde_json::from_value(r.result).unwrap()
    }

    fn profile(summary: &str, want: &str) -> CharacterProfile {
        CharacterProfile {
            summary: summary.into(),
            want: want.into(),
            ..Default::default()
        }
    }

    fn script(text: &str, beat: &str, title: &str) -> ParsedScript {
        let mut p = fountain::parse(text);
        p.scenes[0].story.beat_id = Some(beat.into());
        p.scenes[0].story.title = title.into();
        p
    }

    const V1: &str = "INT. TARMAC - CANTEEN - DAY\n\nPAUL\nHello there.\n\nTHE COLONEL\nAt ease.\n";

    #[test]
    fn newest_wins_without_erasing_and_names_match_cues() {
        let (_d, mut bus) = bus();
        let mut p = script(V1, "b1", "Lunch");
        p.story = StoryContext {
            characters: vec![
                StoryCharacter {
                    name: "Paul".into(),
                    profile: profile("Soldier, lazy", "to chill"),
                },
                StoryCharacter {
                    name: "Ghost".into(),
                    profile: profile("Not in this script", ""),
                },
                StoryCharacter {
                    name: "…".into(),
                    profile: profile("placeholder", ""),
                },
            ],
            locations: vec![
                StoryLocation {
                    name: "TARMAC \u{2013} HALLWAY".into(),
                    description: "The base, 22 floors down.".into(),
                    parent: None,
                },
                StoryLocation {
                    name: "TARMAC - CANTEEN".into(),
                    description: String::new(),
                    parent: Some("TARMAC - HALLWAY".into()),
                },
                StoryLocation {
                    name: "NOWHERE".into(),
                    description: String::new(),
                    parent: None,
                },
            ],
        };
        let s = import(&mut bus, 1, p);
        assert_eq!(s.story.characters_updated, vec!["PAUL"]);
        assert_eq!(s.story.unknown_characters, vec!["Ghost"]);
        let doc = bus.doc();
        assert_eq!(doc.characters["PAUL"].profile.want, "to chill");
        let canteen = &doc.locations["loc_tarmac-canteen"];
        assert_eq!(canteen.parent.as_deref(), Some("loc_tarmac-hallway"));
        assert_eq!(
            doc.locations["loc_tarmac-hallway"].description,
            "The base, 22 floors down."
        );
        assert!(!doc.locations.contains_key("loc_nowhere"));

        // the pilot's bios, context only: COLONEL is THE COLONEL; an empty want keeps the old one
        let r = bus
            .execute(&Command::ImportStoryContext {
                story: StoryContext {
                    characters: vec![
                        StoryCharacter {
                            name: "COLONEL".into(),
                            profile: profile("The boss, Dean Hunnicutt-Price.", "peace and quiet"),
                        },
                        StoryCharacter {
                            name: "Paul".into(),
                            profile: profile("Soldier, friendly idiot", ""),
                        },
                    ],
                    locations: vec![],
                },
            })
            .unwrap();
        let s: StorySummary = serde_json::from_value(r.result).unwrap();
        assert_eq!(s.characters_updated, vec!["THE COLONEL", "PAUL"]);
        let doc = bus.doc();
        assert_eq!(
            doc.characters["PAUL"].profile.summary,
            "Soldier, friendly idiot"
        );
        assert_eq!(doc.characters["PAUL"].profile.want, "to chill");
        assert_eq!(
            doc.characters["THE COLONEL"].profile.one_line(),
            "The boss, Dean Hunnicutt-Price. Wants peace and quiet"
        );
        bus.undo().unwrap();
        assert_eq!(
            bus.doc().characters["PAUL"].profile.summary,
            "Soldier, lazy"
        );
        assert!(bus.doc().characters["THE COLONEL"].profile.is_empty());
    }

    #[test]
    fn beat_id_keeps_the_scene_when_heading_and_lines_change() {
        let (_d, mut bus) = bus();
        import(&mut bus, 1, script(V1, "b1", "Lunch"));
        let id = bus.doc().scenes_in_order()[0].id.clone();
        let v2 = "INT. TARMAC - SITUATION ROOM - NIGHT\n\nNORBERT\nSomething else entirely.\n";
        let s = import(&mut bus, 2, script(v2, "b1", ""));
        assert!(s.scenes_added.is_empty());
        let sc = bus.doc().scenes_in_order()[0].clone();
        assert_eq!(sc.id, id);
        // the newer version had no title: the old one stays
        assert_eq!(sc.story.title, "Lunch");
        assert_eq!(sc.heading, "INT. TARMAC - SITUATION ROOM - NIGHT");
    }

    #[test]
    fn a_setting_without_room_tone_plays_its_parents() {
        let (_d, mut bus) = bus();
        let mut p = script(V1, "b1", "Lunch");
        p.story.locations = vec![StoryLocation {
            name: "TARMAC - CANTEEN".into(),
            description: String::new(),
            parent: Some("TARMAC - HALLWAY".into()),
        }];
        import(&mut bus, 1, p);
        let mut doc = bus.doc().clone();
        let hall = doc.locations.get_mut("loc_tarmac-hallway").unwrap();
        hall.room_tone = RoomTone {
            enabled: true,
            source_id: Some("src".into()),
            t0: 1.0,
            t1: 3.0,
            ..Default::default()
        };
        let tone = doc.room_tone_of("loc_tarmac-canteen").unwrap();
        assert!(tone.enabled && tone.t1 == 3.0);
        let canteen = doc.locations.get_mut("loc_tarmac-canteen").unwrap();
        canteen.room_tone = RoomTone {
            enabled: true,
            t1: 9.0,
            ..Default::default()
        };
        assert_eq!(doc.room_tone_of("loc_tarmac-canteen").unwrap().t1, 9.0);
    }
}
