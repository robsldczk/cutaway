//! Groups files into recording sessions by overlapping timecode (BWF `bext`).
//! Files without timecode that were imported together form one session. Then a director
//! room file whose name points to a character ("Sara-Director_v001") is paired with that
//! character's recordings only ("Sara-Session_v001"), not with the whole folder.

use std::collections::BTreeMap;

use apc_core::Role;

use crate::roles::{character_in_name, tokens, ROOM_KEYWORDS};

#[derive(Debug, Clone, PartialEq)]
pub struct FileSpan {
    pub id: String,
    pub tc_start_s: Option<f64>,
    pub duration_s: f64,
    /// Import batch (files dropped together).
    pub batch: String,
}

/// A group being built: its timecode span (if any), import batch and file ids.
type Group = (Option<(f64, f64)>, String, Vec<String>);

/// Returns groups of file ids.
pub fn group_sessions(files: &[FileSpan]) -> Vec<Vec<String>> {
    let mut groups: Vec<Group> = Vec::new();
    let mut with_tc: Vec<&FileSpan> = files.iter().filter(|f| f.tc_start_s.is_some()).collect();
    with_tc.sort_by(|a, b| {
        a.tc_start_s
            .unwrap_or(0.0)
            .total_cmp(&b.tc_start_s.unwrap_or(0.0))
    });
    for f in with_tc {
        let s = f.tc_start_s.unwrap_or(0.0);
        let e = s + f.duration_s;
        match groups
            .iter_mut()
            .find(|g| g.0.map(|(gs, ge)| s < ge && gs < e).unwrap_or(false))
        {
            Some(g) => {
                if let Some((gs, ge)) = g.0.as_mut() {
                    *gs = gs.min(s);
                    *ge = ge.max(e);
                }
                g.2.push(f.id.clone());
            }
            None => groups.push((Some((s, e)), f.batch.clone(), vec![f.id.clone()])),
        }
    }
    for f in files.iter().filter(|f| f.tc_start_s.is_none()) {
        match groups.iter_mut().find(|g| g.0.is_none() && g.1 == f.batch) {
            Some(g) => g.2.push(f.id.clone()),
            None => groups.push((None, f.batch.clone(), vec![f.id.clone()])),
        }
    }
    groups.into_iter().map(|g| g.2).collect()
}

/// What name pairing needs to know about a file.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedFile {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub character: Option<String>,
}

/// Words that say what a file is, not which take or session it belongs to.
const GENERIC: &[&str] = &[
    "session",
    "rec",
    "recording",
    "mic",
    "track",
    "audio",
    "take",
    "mix",
];

/// Pairs a director room file named after a character with that character's recordings
/// (those sharing the most other name tokens, e.g. "v001", when there are several):
/// from its own group first, else from any group (one side has timecode, the other not).
/// The pairs become sessions of their own; rooms that name nobody stay with what remains
/// of their group (an ensemble room).
pub fn pair_rooms_by_name(
    groups: Vec<Vec<String>>,
    files: &[NamedFile],
    characters: &[String],
    aliases: &BTreeMap<String, String>,
) -> Vec<Vec<String>> {
    let by_id: BTreeMap<&str, &NamedFile> = files.iter().map(|f| (f.id.as_str(), f)).collect();
    let recs_in = |ids: &[String]| -> Vec<&NamedFile> {
        ids.iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .filter(|f| f.role == Role::Character)
            .collect()
    };
    let everyone: Vec<String> = groups.iter().flatten().cloned().collect();
    // room id -> the recordings it names
    let mut targeted: Vec<(String, Vec<String>)> = Vec::new();
    for g in &groups {
        for id in g {
            let Some(room) = by_id.get(id.as_str()).filter(|f| f.role == Role::Room) else {
                continue;
            };
            let Some((ch, _, _)) = character_in_name(&room.name, characters, aliases) else {
                continue;
            };
            let mut mine: Vec<&NamedFile> = recs_in(g);
            mine.retain(|r| r.character.as_deref() == Some(ch.as_str()));
            if mine.is_empty() {
                mine = recs_in(&everyone);
                mine.retain(|r| r.character.as_deref() == Some(ch.as_str()));
            }
            if mine.is_empty() {
                continue;
            }
            let ch_toks = tokens(&ch);
            let distinctive = |name: &str| -> Vec<String> {
                tokens(name)
                    .into_iter()
                    .filter(|t| {
                        !ch_toks.contains(t)
                            && !ROOM_KEYWORDS.contains(&t.as_str())
                            && !GENERIC.contains(&t.as_str())
                    })
                    .collect()
            };
            let room_toks = distinctive(&room.name);
            let score = |r: &NamedFile| {
                distinctive(&r.name)
                    .iter()
                    .filter(|t| room_toks.contains(t))
                    .count()
            };
            let best = mine.iter().map(|r| score(r)).max().unwrap_or(0);
            let chosen = mine
                .iter()
                .filter(|r| best == 0 || score(r) == best)
                .map(|r| r.id.clone())
                .collect();
            targeted.push((id.clone(), chosen));
        }
    }
    if targeted.is_empty() {
        return groups;
    }
    // rooms that share a recording end up in one session
    let mut parts: Vec<Vec<String>> = Vec::new();
    for (room, chosen) in targeted {
        let at = match parts
            .iter()
            .position(|p| chosen.iter().any(|c| p.contains(c)))
        {
            Some(i) => i,
            None => {
                parts.push(Vec::new());
                parts.len() - 1
            }
        };
        for id in chosen.into_iter().chain(std::iter::once(room)) {
            if !parts[at].contains(&id) {
                parts[at].push(id);
            }
        }
    }
    // keep the original order inside each session
    for p in parts.iter_mut() {
        p.sort_by_key(|id| everyone.iter().position(|x| x == id));
    }
    let mut out = parts.clone();
    for g in groups {
        let rest: Vec<String> = g
            .into_iter()
            .filter(|id| !parts.iter().any(|p| p.contains(id)))
            .collect();
        if !rest.is_empty() {
            out.push(rest);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: &str, tc: Option<f64>, d: f64, b: &str) -> FileSpan {
        FileSpan {
            id: id.into(),
            tc_start_s: tc,
            duration_s: d,
            batch: b.into(),
        }
    }

    #[test]
    fn overlapping_timecode_groups() {
        let g = group_sessions(&[
            f("paul", Some(36000.0), 600.0, "a"),
            f("room", Some(36000.2), 600.0, "a"),
            f("sara", Some(50000.0), 300.0, "a"),
            f("x", None, 10.0, "b"),
            f("y", None, 10.0, "b"),
        ]);
        assert_eq!(
            g,
            vec![
                vec!["paul".to_string(), "room".into()],
                vec!["sara".into()],
                vec!["x".into(), "y".into()]
            ]
        );
    }

    fn nf(id: &str, role: Role, ch: Option<&str>) -> NamedFile {
        NamedFile {
            id: id.into(),
            name: format!("{id}.wav"),
            role,
            character: ch.map(String::from),
        }
    }

    #[test]
    fn director_files_pair_with_the_character_they_name() {
        let files = [
            nf("Sara-Session_v001", Role::Character, Some("SARA")),
            nf("Sara-Director_v001", Role::Room, None),
            nf("Sara-Session_v002", Role::Character, Some("SARA")),
            nf("Sara-Director_v002", Role::Room, None),
            nf("Paul-Session_v001", Role::Character, Some("PAUL")),
            nf("Paul-Director_v001", Role::Room, None),
            nf("Norbert_take1", Role::Character, Some("NORBERT")),
            nf("Talkback_group", Role::Room, None),
        ];
        let all: Vec<String> = files.iter().map(|f| f.id.clone()).collect();
        let chars = vec!["SARA".to_string(), "PAUL".into(), "NORBERT".into()];
        let g = pair_rooms_by_name(vec![all], &files, &chars, &BTreeMap::new());
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            g,
            vec![
                s(&["Sara-Session_v001", "Sara-Director_v001"]),
                s(&["Sara-Session_v002", "Sara-Director_v002"]),
                s(&["Paul-Session_v001", "Paul-Director_v001"]),
                s(&["Norbert_take1", "Talkback_group"]),
            ]
        );
    }

    #[test]
    fn a_named_room_finds_its_recording_in_another_group() {
        // the recordings have timecode, the director files not: two groups by grouping
        let files = [
            nf("Sara-Session_v001", Role::Character, Some("SARA")),
            nf("Paul-Session_v001", Role::Character, Some("PAUL")),
            nf("Sara-Director_v001", Role::Room, None),
            nf("Paul-Director_v001", Role::Room, None),
        ];
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let groups = vec![
            s(&["Sara-Session_v001", "Paul-Session_v001"]),
            s(&["Sara-Director_v001", "Paul-Director_v001"]),
        ];
        let chars = vec!["SARA".to_string(), "PAUL".into()];
        let g = pair_rooms_by_name(groups, &files, &chars, &BTreeMap::new());
        assert_eq!(
            g,
            vec![
                s(&["Sara-Session_v001", "Sara-Director_v001"]),
                s(&["Paul-Session_v001", "Paul-Director_v001"]),
            ]
        );
    }

    #[test]
    fn rooms_naming_nobody_keep_the_ensemble() {
        let files = [
            nf("Paul", Role::Character, Some("PAUL")),
            nf("Sara", Role::Character, Some("SARA")),
            nf("Room", Role::Room, None),
        ];
        let all: Vec<String> = files.iter().map(|f| f.id.clone()).collect();
        let chars = vec!["SARA".to_string(), "PAUL".into()];
        let g = pair_rooms_by_name(vec![all.clone()], &files, &chars, &BTreeMap::new());
        assert_eq!(g, vec![all]);
    }
}
