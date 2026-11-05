//! Script versions: store each import as `Script/vNNN <name>.fountain` (or `.thrl`), diff
//! it against the current version and keep line IDs (and their takes and picks) for
//! unchanged lines. Story context (scene titles, synopses, bios) merges newest-wins.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::bus::Tx;
use crate::error::{CoreError, Result};
use crate::fountain::{ParsedScript, ParsedUnit};
use crate::ids::new_id;
use crate::model::{
    BoxState, Character, LineKind, Location, Scene, ScriptLine, ScriptVersion, Spacer, Take,
    Unmatched,
};
use crate::project::EpisodePaths;
use crate::story::StorySummary;
use crate::text::{display_tokens, norm_word};

/// One line in the change summary.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, schemars::JsonSchema)]
pub struct LineChange {
    pub line_id: String,
    pub character: Option<String>,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
}

/// What a version switch changes; shown before it applies.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, schemars::JsonSchema)]
pub struct ChangeSummary {
    pub version: u32,
    pub unchanged: usize,
    pub changed: Vec<LineChange>,
    pub inserted: Vec<LineChange>,
    pub deleted: Vec<LineChange>,
    pub scenes_added: Vec<String>,
    pub scenes_removed: Vec<String>,
    pub takes_to_unmatched: usize,
    pub takes_to_rematch: usize,
    #[serde(default)]
    pub story: StorySummary,
}

impl ChangeSummary {
    pub fn text(&self) -> String {
        format!(
            "v{:03}: {} unchanged, {} changed, {} new, {} deleted lines; {} scenes added, {} removed; {} takes to re-match, {} moved to the unmatched bin",
            self.version,
            self.unchanged,
            self.changed.len(),
            self.inserted.len(),
            self.deleted.len(),
            self.scenes_added.len(),
            self.scenes_removed.len(),
            self.takes_to_rematch,
            self.takes_to_unmatched
        ) + &story_text(&self.story)
    }
}

fn story_text(s: &StorySummary) -> String {
    let mut parts = Vec::new();
    if !s.characters_updated.is_empty() {
        parts.push(format!("{} character notes", s.characters_updated.len()));
    }
    if !s.locations_updated.is_empty() {
        parts.push(format!("{} settings", s.locations_updated.len()));
    }
    let mut out = String::new();
    if !parts.is_empty() {
        out = format!("; story context: {} updated", parts.join(", "));
    }
    if !s.unknown_characters.is_empty() {
        out.push_str(&format!(
            "; no script character for {}",
            s.unknown_characters.join(", ")
        ));
    }
    out
}

/// Reads a script file: a Throughline `.thrl` project (by content) or Fountain text.
pub fn read_script_file(path: &Path) -> Result<ParsedScript> {
    let bytes = std::fs::read(path)?;
    if crate::thrl::looks_like_thrl(&bytes) {
        return crate::thrl::parse(&bytes);
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        CoreError::invalid(format!(
            "{} is neither a Fountain text nor a Throughline project",
            path.display()
        ))
    })?;
    Ok(crate::fountain::parse(&text))
}

/// A script file as Fountain text (a `.thrl` project is rendered).
pub fn script_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    if crate::thrl::looks_like_thrl(&bytes) {
        return Ok(crate::fountain::render(&crate::thrl::parse(&bytes)?));
    }
    String::from_utf8(bytes).map_err(|e| CoreError::invalid(e.to_string()))
}

/// Copies a script into `Script/` as the next version. Old versions are never overwritten.
pub fn store_version_file(
    paths: &EpisodePaths,
    src: &Path,
    name: &str,
    number: u32,
) -> Result<(String, String)> {
    std::fs::create_dir_all(&paths.script)?;
    let safe: String = name
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let bytes = std::fs::read(src)?;
    let ext = if crate::thrl::looks_like_thrl(&bytes) {
        "thrl"
    } else {
        "fountain"
    };
    let file_name = format!("v{number:03} {safe}.{ext}");
    let dest = paths.script.join(&file_name);
    std::fs::write(&dest, &bytes)?;
    let hash = crate::media::hash_file(&dest)?;
    Ok((format!("Script/{file_name}"), hash))
}

fn unit_key(kind: LineKind, character: Option<&str>, text: &str) -> String {
    let words: Vec<String> = display_tokens(text).iter().map(|t| norm_word(t)).collect();
    format!("{:?}|{}|{}", kind, character.unwrap_or(""), words.join(" "))
}

fn word_similarity(a: &str, b: &str) -> f64 {
    let wa: Vec<String> = display_tokens(a).iter().map(|t| norm_word(t)).collect();
    let wb: Vec<String> = display_tokens(b).iter().map(|t| norm_word(t)).collect();
    if wa.is_empty() && wb.is_empty() {
        return 1.0;
    }
    let l = lcs_len(&wa, &wb);
    2.0 * l as f64 / (wa.len() + wb.len()) as f64
}

fn lcs_len<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut prev = vec![0usize; b.len() + 1];
    let mut cur = vec![0usize; b.len() + 1];
    for x in a {
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y {
                prev[j] + 1
            } else {
                cur[j].max(prev[j + 1])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// LCS alignment of two key sequences.
fn align(old: &[String], new: &[String]) -> Vec<Op> {
    let (n, m) = (old.len(), new.len());
    let mut dp = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[at(i, j)] = if old[i] == new[j] {
                dp[at(i + 1, j + 1)] + 1
            } else {
                dp[at(i + 1, j)].max(dp[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut ops = Vec::with_capacity(n.max(m));
    while i < n && j < m {
        if old[i] == new[j] {
            ops.push(Op::Equal(i, j));
            i += 1;
            j += 1;
        } else if dp[at(i + 1, j)] >= dp[at(i, j + 1)] {
            ops.push(Op::Delete(i));
            i += 1;
        } else {
            ops.push(Op::Insert(j));
            j += 1;
        }
    }
    ops.extend((i..n).map(Op::Delete));
    ops.extend((j..m).map(Op::Insert));
    ops
}

pub fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Short place name for display: "TARMAC - CANTEEN" -> "Canteen".
pub fn short_place(location: &str) -> String {
    let last = location.rsplit(" - ").next().unwrap_or(location);
    title_case(last)
}

pub fn location_id(location: &str) -> String {
    let mut s = String::new();
    for c in location.to_lowercase().chars() {
        if c.is_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    format!("loc_{}", s.trim_matches('-'))
}

/// Applies a parsed script version to the document inside a command.
pub fn apply_version(
    tx: &mut Tx,
    version: &ScriptVersion,
    parsed: &ParsedScript,
) -> Result<ChangeSummary> {
    let doc = tx.doc().clone();
    let mut summary = ChangeSummary {
        version: version.number,
        ..Default::default()
    };

    // Old units in script order (ad-libs are not part of the script).
    let mut old_lines: Vec<&ScriptLine> = doc.lines.values().filter(|l| !l.adlib).collect();
    old_lines.sort_by_key(|l| {
        (
            doc.scenes.get(&l.scene_id).map(|s| s.number).unwrap_or(0),
            l.number,
        )
    });
    let old_keys: Vec<String> = old_lines
        .iter()
        .map(|l| unit_key(l.kind, l.character.as_deref(), &l.text))
        .collect();

    // New units, flattened.
    let mut new_units: Vec<(usize, &ParsedUnit)> = Vec::new();
    for (si, sc) in parsed.scenes.iter().enumerate() {
        for u in &sc.units {
            new_units.push((si, u));
        }
    }
    let new_keys: Vec<String> = new_units
        .iter()
        .map(|(_, u)| unit_key(u.kind, u.character.as_deref(), &u.text))
        .collect();

    // Align, then pair deletes and inserts inside each hunk as "changed".
    let ops = align(&old_keys, &new_keys);
    let mut new_to_old: HashMap<usize, (usize, bool)> = HashMap::new(); // new -> (old, changed)
    let mut deleted: Vec<usize> = Vec::new();
    let mut k = 0;
    while k < ops.len() {
        match ops[k] {
            Op::Equal(o, n) => {
                new_to_old.insert(n, (o, false));
                k += 1;
            }
            _ => {
                let mut dels = Vec::new();
                let mut ins = Vec::new();
                while k < ops.len() {
                    match ops[k] {
                        Op::Delete(o) => dels.push(o),
                        Op::Insert(n) => ins.push(n),
                        Op::Equal(..) => break,
                    }
                    k += 1;
                }
                let mut used = vec![false; dels.len()];
                for n in ins {
                    let nu = new_units[n].1;
                    let best = dels
                        .iter()
                        .enumerate()
                        .filter(|(di, _)| !used[*di])
                        .filter(|(_, o)| {
                            let ol = old_lines[**o];
                            ol.kind == nu.kind && ol.character == nu.character
                        })
                        .map(|(di, o)| (di, *o, word_similarity(&old_lines[*o].text, &nu.text)))
                        .filter(|(_, _, s)| *s >= 0.5)
                        .max_by(|a, b| a.2.total_cmp(&b.2));
                    if let Some((di, o, _)) = best {
                        used[di] = true;
                        new_to_old.insert(n, (o, true));
                    }
                }
                for (di, o) in dels.into_iter().enumerate() {
                    if !used[di] {
                        deleted.push(o);
                    }
                }
            }
        }
    }

    // Scene ids: keep the old scene that holds most of the new scene's kept lines,
    // else one with the same heading.
    let mut scene_ids: Vec<String> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    for (si, sc) in parsed.scenes.iter().enumerate() {
        let mut votes: BTreeMap<String, usize> = BTreeMap::new();
        for (n, (nsi, _)) in new_units.iter().enumerate() {
            if *nsi != si {
                continue;
            }
            if let Some((o, _)) = new_to_old.get(&n) {
                *votes.entry(old_lines[*o].scene_id.clone()).or_default() += 1;
            }
        }
        let by_vote = votes
            .into_iter()
            .filter(|(id, _)| !taken.contains(id))
            .max_by_key(|(_, c)| *c)
            .map(|(id, _)| id);
        let by_beat = || {
            let beat = sc.story.beat_id.as_deref()?;
            doc.scenes
                .values()
                .find(|s| s.story.beat_id.as_deref() == Some(beat) && !taken.contains(&s.id))
                .map(|s| s.id.clone())
        };
        let by_heading = || {
            doc.scenes
                .values()
                .find(|s| s.heading == sc.heading && !taken.contains(&s.id))
                .map(|s| s.id.clone())
        };
        let id = by_vote
            .or_else(by_beat)
            .or_else(by_heading)
            .unwrap_or_else(|| {
                summary.scenes_added.push(sc.heading.clone());
                new_id("sc")
            });
        taken.insert(id.clone());
        scene_ids.push(id);
    }

    // Characters and locations.
    let mut next_color = doc
        .characters
        .values()
        .map(|c| c.color_index + 1)
        .max()
        .unwrap_or(0);
    for name in parsed.characters() {
        if !doc.characters.contains_key(&name) {
            tx.put(&Character {
                id: name.clone(),
                label: title_case(&name),
                aliases: Vec::new(),
                color_index: next_color,
                profile: Default::default(),
                wet: crate::model::DEFAULT_VOICE_WET,
            })?;
            next_color += 1;
        }
    }
    for sc in &parsed.scenes {
        let lid = location_id(&sc.location);
        if !sc.location.is_empty() && !doc.locations.contains_key(&lid) {
            tx.put(&Location {
                id: lid,
                name: sc.location.clone(),
                room_tone: Default::default(),
                description: String::new(),
                parent: None,
            })?;
        }
    }
    summary.story = crate::story::merge(tx, &parsed.story)?;

    // Lines.
    let mut orders: Vec<Vec<String>> = vec![Vec::new(); parsed.scenes.len()];
    let mut numbers: Vec<u32> = vec![0; parsed.scenes.len()];
    for (n, (si, u)) in new_units.iter().enumerate() {
        numbers[*si] += 1;
        let tokens = display_tokens(&u.text);
        let words = tokens.iter().map(|t| norm_word(t)).collect();
        let (id, changed, version_added) = match new_to_old.get(&n) {
            Some((o, changed)) => {
                let ol = old_lines[*o];
                (ol.id.clone(), *changed, ol.version_added)
            }
            None => (new_id("ln"), false, version.number),
        };
        let line = ScriptLine {
            id: id.clone(),
            scene_id: scene_ids[*si].clone(),
            kind: u.kind,
            character: u.character.clone(),
            extension: u.extension.clone(),
            text: u.text.clone(),
            paren: u.paren.clone(),
            tokens,
            words,
            number: numbers[*si],
            version_added,
            changed,
            forced: u.forced,
            adlib: false,
            // a deleted line stays deleted, whatever the new version says
            removed: new_to_old
                .get(&n)
                .is_some_and(|(o, _)| old_lines[*o].removed),
        };
        let entry = LineChange {
            line_id: id.clone(),
            character: u.character.clone(),
            old_text: new_to_old.get(&n).map(|(o, _)| old_lines[*o].text.clone()),
            new_text: Some(u.text.clone()),
        };
        match new_to_old.get(&n) {
            Some((_, true)) => {
                // re-match: keep the takes, flag them, keep the old pick as a suggestion
                for t in doc.takes.values().filter(|t| t.line_id == id) {
                    let mut t2: Take = t.clone();
                    t2.needs_rematch = true;
                    tx.put(&t2)?;
                    summary.takes_to_rematch += 1;
                }
                if let Some(b) = doc.boxes.get(&id) {
                    let mut b2 = b.clone();
                    b2.suggested_take = b.take_id.clone();
                    b2.comp.clear();
                    b2.edits = Default::default();
                    tx.put(&b2)?;
                }
                summary.changed.push(entry);
            }
            Some((_, false)) => summary.unchanged += 1,
            None => summary.inserted.push(entry),
        }
        match u.kind {
            LineKind::Dialogue => {
                if !doc.boxes.contains_key(&id) {
                    tx.put(&BoxState {
                        id: id.clone(),
                        ..Default::default()
                    })?;
                }
            }
            _ => {
                if !doc.spacers.contains_key(&id) {
                    tx.put(&Spacer {
                        id: id.clone(),
                        duration_s: None,
                        enabled: u.kind == LineKind::Action,
                    })?;
                }
            }
        }
        let in_cut = !line.removed;
        tx.put(&line)?;
        if in_cut {
            orders[*si].push(id);
        }
    }

    // Deleted lines: their takes go to the unmatched bin.
    for o in deleted {
        let ol = old_lines[o];
        summary.deleted.push(LineChange {
            line_id: ol.id.clone(),
            character: ol.character.clone(),
            old_text: Some(ol.text.clone()),
            new_text: None,
        });
        let scene_still = scene_ids.contains(&ol.scene_id);
        for t in doc.takes.values().filter(|t| t.line_id == ol.id) {
            tx.put(&Unmatched {
                id: new_id("um"),
                scene_id: if scene_still {
                    Some(ol.scene_id.clone())
                } else {
                    None
                },
                source_id: t.source_id.clone(),
                t0: t.t0,
                t1: t.t1,
                text: if t.heard.is_empty() {
                    ol.text.clone()
                } else {
                    t.heard.clone()
                },
                db: t.db,
                origin: "deleted_line".into(),
                used_as: None,
            })?;
            tx.delete::<Take>(&t.id)?;
            summary.takes_to_unmatched += 1;
        }
        tx.delete::<BoxState>(&ol.id)?;
        tx.delete::<Spacer>(&ol.id)?;
        tx.delete::<ScriptLine>(&ol.id)?;
    }

    // Scenes (order resets to script order for the new version; ad-libs stay at their place).
    for (si, sc) in parsed.scenes.iter().enumerate() {
        let id = &scene_ids[si];
        let old = doc.scenes.get(id);
        let mut order = orders[si].clone();
        if let Some(old) = old {
            for (pos, item) in old.order.iter().enumerate() {
                if doc.lines.get(item).map(|l| l.adlib).unwrap_or(false) {
                    order.insert(pos.min(order.len()), item.clone());
                }
            }
        }
        tx.put(&Scene {
            id: id.clone(),
            number: (si + 1) as u32,
            heading: sc.heading.clone(),
            int_ext: sc.int_ext.clone(),
            location: sc.location.clone(),
            place: short_place(&sc.location),
            time_of_day: title_case(&sc.time_of_day),
            order,
            tightness: old.map(|s| s.tightness).unwrap_or(1.0),
            story: crate::story::merge_scene(old.map(|s| &s.story), &sc.story),
            room: old.map(|s| s.room).unwrap_or_default(),
            tone: old.and_then(|s| s.tone.clone()),
        })?;
    }
    for s in doc.scenes.values() {
        if !scene_ids.contains(&s.id) {
            summary.scenes_removed.push(s.heading.clone());
            // ad-libs of a removed scene go back to the bin
            for l in doc.lines.values().filter(|l| l.adlib && l.scene_id == s.id) {
                tx.delete::<ScriptLine>(&l.id)?;
                tx.delete::<BoxState>(&l.id)?;
            }
            tx.delete::<Scene>(&s.id)?;
        }
    }

    let mut v = version.clone();
    v.summary = serde_json::to_value(&summary)?;
    tx.put(&v)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::CommandBus;
    use crate::commands::Command;
    use crate::fountain;
    use crate::model::TakeType;
    use crate::store::EpisodeStore;

    const V1: &str = "INT. LAB - DAY\n\nPAUL\nHello there.\n\nSARA\nHi Paul.\n\nPaul waves.\n\nPAUL\nGoodbye now.\n\nSARA\nSee you tomorrow.\n";

    fn version(n: u32) -> ScriptVersion {
        ScriptVersion {
            id: format!("sv_{n}"),
            number: n,
            file: format!("Script/v{n:03} test.fountain"),
            name: "test".into(),
            hash: String::new(),
            imported_at: String::new(),
            summary: serde_json::Value::Null,
            title: Default::default(),
        }
    }

    fn bus() -> (tempfile::TempDir, CommandBus) {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::create(&dir.path().join("t.cutaway")).unwrap();
        (dir, CommandBus::new(store, "test").unwrap())
    }

    fn import(bus: &mut CommandBus, n: u32, text: &str) -> ChangeSummary {
        let r = bus
            .execute(&Command::ImportScriptVersion {
                version: version(n),
                parsed: fountain::parse(text),
            })
            .unwrap();
        serde_json::from_value(r.result).unwrap()
    }

    fn id_of(bus: &CommandBus, text: &str) -> String {
        bus.doc()
            .lines
            .values()
            .find(|l| l.text == text)
            .unwrap()
            .id
            .clone()
    }

    fn add_take(bus: &mut CommandBus, line_id: &str) {
        let take = Take {
            id: format!("tk_{line_id}"),
            line_id: line_id.into(),
            source_id: "src_1".into(),
            take_no: 1,
            take_type: TakeType::Full,
            t0: 1.0,
            t1: 2.0,
            words: vec![],
            score: 100.0,
            heard: "heard text".into(),
            lufs: None,
            db: -20.0,
            partial: false,
            pick: 0,
            favorite: false,
            favorite_origin: None,
            direction: String::new(),
            feedback: String::new(),
            direction_raw: String::new(),
            feedback_raw: String::new(),
            tags: vec![],
            note_ids: vec![],
            confidence: 1.0,
            needs_rematch: false,
            ai_flag: None,
            origin: Default::default(),
            read_as: None,
        };
        bus.execute_fn("test_add_take", &serde_json::Value::Null, |tx| {
            tx.put(&take)?;
            Ok(serde_json::Value::Null)
        })
        .unwrap();
    }

    #[test]
    fn first_import_inserts_everything() {
        let (_d, mut bus) = bus();
        let s = import(&mut bus, 1, V1);
        assert_eq!(s.inserted.len(), 5);
        assert_eq!(bus.doc().scenes.len(), 1);
        assert_eq!(bus.doc().characters.len(), 2);
        assert_eq!(bus.doc().boxes.len(), 4);
        assert_eq!(bus.doc().spacers.len(), 1);
    }

    #[test]
    fn edit_a_line_keeps_ids_and_flags_rematch() {
        let (_d, mut bus) = bus();
        import(&mut bus, 1, V1);
        let hello = id_of(&bus, "Hello there.");
        let goodbye = id_of(&bus, "Goodbye now.");
        add_take(&mut bus, &goodbye);
        let v2 = V1.replace("Goodbye now.", "Goodbye for now.");
        let s = import(&mut bus, 2, &v2);
        assert_eq!(s.unchanged, 4);
        assert_eq!(s.changed.len(), 1);
        assert_eq!(id_of(&bus, "Hello there."), hello);
        assert_eq!(id_of(&bus, "Goodbye for now."), goodbye);
        let t = &bus.doc().takes[&format!("tk_{goodbye}")];
        assert!(t.needs_rematch);
        assert!(bus.doc().lines[&goodbye].changed);
    }

    #[test]
    fn delete_a_line_moves_takes_to_unmatched() {
        let (_d, mut bus) = bus();
        import(&mut bus, 1, V1);
        let hi = id_of(&bus, "Hi Paul.");
        add_take(&mut bus, &hi);
        let v2 = V1.replace("SARA\nHi Paul.\n\n", "");
        let s = import(&mut bus, 2, &v2);
        assert_eq!(s.deleted.len(), 1);
        assert_eq!(s.takes_to_unmatched, 1);
        assert!(!bus.doc().lines.contains_key(&hi));
        assert!(bus.doc().takes.is_empty());
        let um: Vec<_> = bus.doc().unmatched.values().collect();
        assert_eq!(um.len(), 1);
        assert_eq!(um[0].origin, "deleted_line");
        assert_eq!(um[0].text, "heard text");
    }

    #[test]
    fn a_new_version_leaves_a_deleted_line_deleted() {
        let (_d, mut bus) = bus();
        import(&mut bus, 1, V1);
        let hi = id_of(&bus, "Hi Paul.");
        bus.execute(&Command::DeleteLine {
            line_id: hi.clone(),
        })
        .unwrap();
        let v2 = V1.replace("Goodbye now.", "Goodbye for now.");
        import(&mut bus, 2, &v2);
        let doc = bus.doc();
        assert!(doc.lines[&hi].removed);
        assert!(doc.scenes.values().all(|s| !s.order.contains(&hi)));
    }

    #[test]
    fn insert_a_line_gets_a_new_id_and_keeps_others() {
        let (_d, mut bus) = bus();
        import(&mut bus, 1, V1);
        let before: HashSet<String> = bus.doc().lines.keys().cloned().collect();
        let v2 = V1.replace("Paul waves.\n", "Paul waves.\n\nPAUL\nOne more thing.\n");
        let s = import(&mut bus, 2, &v2);
        assert_eq!(s.inserted.len(), 1);
        assert_eq!(s.unchanged, 5);
        let after: HashSet<String> = bus.doc().lines.keys().cloned().collect();
        assert!(before.is_subset(&after));
        let new_line = &bus.doc().lines[&id_of(&bus, "One more thing.")];
        assert_eq!(new_line.version_added, 2);
        let scene = bus.doc().scenes.values().next().unwrap();
        assert_eq!(scene.order.len(), 6);
        assert_eq!(scene.order[3], new_line.id);
    }

    #[test]
    fn version_import_is_undoable() {
        let (_d, mut bus) = bus();
        import(&mut bus, 1, V1);
        let snapshot = bus.doc().clone();
        import(&mut bus, 2, &V1.replace("Hello there.", "Hello you."));
        bus.undo().unwrap();
        assert_eq!(bus.doc(), &snapshot);
    }

    #[test]
    fn places_are_short() {
        assert_eq!(short_place("TARMAC - CANTEEN"), "Canteen");
        assert_eq!(
            location_id("TARMAC - COLONEL\u{2019}S OFFICE"),
            "loc_tarmac-colonel-s-office"
        );
    }
}
