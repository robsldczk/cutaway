//! Autosave snapshots, rotation, named versions and revert.
//!
//! Every command is already on disk (SQLite WAL). On top of that, a full snapshot goes to
//! `Backups/` every 10 minutes while changes happen; the last 20 are kept plus one per day.
//! Named versions are never rotated. Revert first snapshots the current state.

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::params;
use serde::Serialize;

use crate::brand;
use crate::bus::CommandBus;
use crate::error::{CoreError, Result};
use crate::ids::new_id;
use crate::project::EpisodePaths;
use crate::store::EpisodeStore;

pub trait Clock: Send {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RotationPolicy {
    pub interval: Duration,
    pub keep_last: usize,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        RotationPolicy {
            interval: Duration::minutes(10),
            keep_last: 20,
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct BackupInfo {
    pub id: String,
    /// `auto`, `named` or `pre-revert`.
    pub kind: String,
    pub name: Option<String>,
    /// File name inside `Backups/`.
    pub file: String,
    pub created_at: String,
    pub seq: Option<i64>,
}

pub struct BackupManager {
    dir: PathBuf,
    policy: RotationPolicy,
    clock: Box<dyn Clock>,
    last_auto: DateTime<Utc>,
    dirty: bool,
}

impl BackupManager {
    pub fn new(paths: &EpisodePaths, policy: RotationPolicy, clock: Box<dyn Clock>) -> Self {
        let last_auto = clock.now();
        BackupManager {
            dir: paths.backups.clone(),
            policy,
            clock,
            last_auto,
            dirty: false,
        }
    }

    /// Call after every write.
    pub fn note_change(&mut self) {
        self.dirty = true;
    }

    /// Takes an auto snapshot when changes happened and the interval has passed.
    pub fn maybe_autosave(&mut self, store: &EpisodeStore) -> Result<Option<BackupInfo>> {
        let now = self.clock.now();
        if !self.dirty || now - self.last_auto < self.policy.interval {
            return Ok(None);
        }
        let info = self.snapshot(store, "auto", None)?;
        self.rotate(store)?;
        self.last_auto = now;
        self.dirty = false;
        Ok(Some(info))
    }

    /// Takes an auto snapshot now if anything changed since the last one (closing an episode,
    /// quitting the app), so the last minutes of work are in `Backups/` too.
    pub fn flush(&mut self, store: &EpisodeStore) -> Result<Option<BackupInfo>> {
        if !self.dirty {
            return Ok(None);
        }
        let info = self.snapshot(store, "auto", None)?;
        self.rotate(store)?;
        self.last_auto = self.clock.now();
        self.dirty = false;
        Ok(Some(info))
    }

    /// Writes a full copy of the episode file into `Backups/`.
    pub fn snapshot(
        &self,
        store: &EpisodeStore,
        kind: &str,
        name: Option<&str>,
    ) -> Result<BackupInfo> {
        std::fs::create_dir_all(&self.dir)?;
        let now = self.clock.now();
        let stamp = now.format("%Y-%m-%d %H%M%S%.3f").to_string();
        let label = match (kind, name) {
            ("named", Some(n)) => sanitize(n),
            (k, _) => k.to_string(),
        };
        let file = format!("{stamp} {label}.{}", brand::EPISODE_EXT);
        let path = self.dir.join(&file);
        if path.exists() {
            return Err(CoreError::invalid(format!("{} exists", path.display())));
        }
        let path_str = path.to_string_lossy().into_owned();
        store.conn().execute("VACUUM INTO ?1", [path_str])?;
        let seq: Option<i64> = store
            .conn()
            .query_row("SELECT MAX(seq) FROM command_log", [], |r| r.get(0))
            .unwrap_or(None);
        let info = BackupInfo {
            id: new_id("bk"),
            kind: kind.to_string(),
            name: name.map(str::to_string),
            file,
            created_at: now.to_rfc3339(),
            seq,
        };
        store.conn().execute(
            "INSERT INTO backups(id,kind,name,file,created_at,seq) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                info.id,
                info.kind,
                info.name,
                info.file,
                info.created_at,
                info.seq
            ],
        )?;
        Ok(info)
    }

    pub fn save_version(&self, store: &EpisodeStore, name: &str) -> Result<BackupInfo> {
        if name.trim().is_empty() {
            return Err(CoreError::invalid("a version needs a name"));
        }
        self.snapshot(store, "named", Some(name.trim()))
    }

    pub fn list(store: &EpisodeStore) -> Result<Vec<BackupInfo>> {
        let mut stmt = store.conn().prepare(
            "SELECT id, kind, name, file, created_at, seq FROM backups ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(BackupInfo {
                id: r.get(0)?,
                kind: r.get(1)?,
                name: r.get(2)?,
                file: r.get(3)?,
                created_at: r.get(4)?,
                seq: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Deletes auto snapshots beyond the policy. Returns the deleted ids.
    pub fn rotate(&self, store: &EpisodeStore) -> Result<Vec<String>> {
        let autos: Vec<(String, DateTime<Utc>, String)> = Self::list(store)?
            .into_iter()
            .filter(|b| b.kind == "auto")
            .filter_map(|b| {
                DateTime::parse_from_rfc3339(&b.created_at)
                    .ok()
                    .map(|t| (b.id, t.with_timezone(&Utc), b.file))
            })
            .collect();
        let pairs: Vec<(String, DateTime<Utc>)> =
            autos.iter().map(|(i, t, _)| (i.clone(), *t)).collect();
        let doomed = select_for_deletion(&pairs, self.policy.keep_last);
        for (id, _, file) in autos.iter().filter(|(i, _, _)| doomed.contains(i)) {
            let p = self.dir.join(file);
            if p.exists() {
                std::fs::remove_file(&p)?;
            }
            store
                .conn()
                .execute("DELETE FROM backups WHERE id=?1", [id])?;
        }
        Ok(doomed)
    }

    /// Replaces the document with a backup. Snapshots the current state first.
    pub fn revert(&self, bus: &mut CommandBus, backup_id: &str) -> Result<BackupInfo> {
        let all = Self::list(bus.store())?;
        let target = all
            .iter()
            .find(|b| b.id == backup_id)
            .cloned()
            .ok_or_else(|| CoreError::not_found("backup", backup_id))?;
        let src = self.dir.join(&target.file);
        if !src.exists() {
            return Err(CoreError::not_found(
                "backup file",
                src.display().to_string(),
            ));
        }
        let pre = self.snapshot(bus.store(), "pre-revert", None)?;
        let conn = bus.store().conn();
        conn.execute(
            "ATTACH DATABASE ?1 AS bak",
            [src.to_string_lossy().into_owned()],
        )?;
        let result = (|| -> Result<()> {
            conn.execute_batch(
                "BEGIN IMMEDIATE;
                 DELETE FROM main.entity;
                 INSERT INTO main.entity(kind,id,parent,ord,data) SELECT kind,id,parent,ord,data FROM bak.entity;
                 DELETE FROM main.command_log;
                 INSERT INTO main.command_log SELECT * FROM bak.command_log;
                 COMMIT;",
            )?;
            Ok(())
        })();
        if result.is_err() {
            let _ = conn.execute_batch("ROLLBACK");
        }
        conn.execute("DETACH DATABASE bak", [])?;
        result?;
        bus.store().bump_rev()?;
        bus.reload()?;
        Ok(pre)
    }
}

/// Auto backups to delete: everything beyond the newest `keep_last`, except the newest
/// backup of each day.
pub fn select_for_deletion(autos: &[(String, DateTime<Utc>)], keep_last: usize) -> Vec<String> {
    let mut sorted: Vec<&(String, DateTime<Utc>)> = autos.iter().collect();
    sorted.sort_by_key(|x| std::cmp::Reverse(x.1));
    let mut days: HashSet<NaiveDate> = HashSet::new();
    let mut doomed = Vec::new();
    for (i, (id, t)) in sorted.iter().enumerate() {
        let day = t.date_naive();
        let first_of_day = days.insert(day);
        if i < keep_last || first_of_day {
            continue;
        }
        doomed.push(id.clone());
    }
    doomed
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Command;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeClock(Arc<Mutex<DateTime<Utc>>>);
    impl Clock for FakeClock {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap()
        }
    }
    impl FakeClock {
        fn advance(&self, d: Duration) {
            let mut t = self.0.lock().unwrap();
            *t += d;
        }
    }

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn rotation_keeps_last_20_and_one_per_day() {
        let mut autos = Vec::new();
        // 3 days x 30 backups, 10 minutes apart
        for day in 1..=3 {
            for k in 0..30 {
                let ts = t(&format!("2026-10-0{day}T08:00:00Z")) + Duration::minutes(10 * k);
                autos.push((format!("d{day}-{k:02}"), ts));
            }
        }
        let doomed = select_for_deletion(&autos, 20);
        let kept: Vec<&String> = autos
            .iter()
            .map(|(i, _)| i)
            .filter(|i| !doomed.contains(i))
            .collect();
        assert_eq!(kept.len(), 22);
        for k in 10..30 {
            assert!(kept.contains(&&format!("d3-{k:02}")));
        }
        assert!(kept.contains(&&"d2-29".to_string()));
        assert!(kept.contains(&&"d1-29".to_string()));
    }

    fn setup() -> (tempfile::TempDir, EpisodePaths, CommandBus, FakeClock) {
        let dir = tempfile::tempdir().unwrap();
        let (paths, store) = crate::project::create_episode(dir.path(), "Ep", None).unwrap();
        let bus = CommandBus::new(store, "test").unwrap();
        let clock = FakeClock(Arc::new(Mutex::new(t("2026-10-31T09:00:00Z"))));
        (dir, paths, bus, clock)
    }

    #[test]
    fn autosave_only_while_changing_and_after_interval() {
        let (_d, paths, mut bus, clock) = setup();
        let mut bm = BackupManager::new(&paths, RotationPolicy::default(), Box::new(clock.clone()));
        clock.advance(Duration::minutes(11));
        assert!(
            bm.maybe_autosave(bus.store()).unwrap().is_none(),
            "no change, no backup"
        );
        bus.execute(&Command::SetConfig {
            key: "ai.model".into(),
            value: "m1".into(),
        })
        .unwrap();
        bm.note_change();
        clock.advance(Duration::minutes(5));
        // 16 minutes since the manager started: due
        let b = bm.maybe_autosave(bus.store()).unwrap().unwrap();
        assert!(paths.backups.join(&b.file).exists());
        bm.note_change();
        clock.advance(Duration::minutes(3));
        assert!(
            bm.maybe_autosave(bus.store()).unwrap().is_none(),
            "interval not reached"
        );
        clock.advance(Duration::minutes(8));
        assert!(bm.maybe_autosave(bus.store()).unwrap().is_some());
        assert_eq!(BackupManager::list(bus.store()).unwrap().len(), 2);
    }

    #[test]
    fn flush_snapshots_only_unsaved_changes() {
        let (_d, paths, mut bus, clock) = setup();
        let mut bm = BackupManager::new(&paths, RotationPolicy::default(), Box::new(clock));
        assert!(bm.flush(bus.store()).unwrap().is_none(), "nothing changed");
        bus.execute(&Command::SetConfig {
            key: "ai.model".into(),
            value: "m1".into(),
        })
        .unwrap();
        bm.note_change();
        assert!(bm.flush(bus.store()).unwrap().is_some());
        assert!(bm.flush(bus.store()).unwrap().is_none(), "already saved");
    }

    #[test]
    fn autosave_rotates_files_on_disk() {
        let (_d, paths, bus, clock) = setup();
        let policy = RotationPolicy {
            interval: Duration::minutes(10),
            keep_last: 3,
        };
        let mut bm = BackupManager::new(&paths, policy, Box::new(clock.clone()));
        for _ in 0..6 {
            bm.note_change();
            clock.advance(Duration::minutes(10));
            bm.maybe_autosave(bus.store()).unwrap().unwrap();
        }
        let list = BackupManager::list(bus.store()).unwrap();
        assert_eq!(list.len(), 3, "same day: only the last 3 survive");
        let files = std::fs::read_dir(&paths.backups).unwrap().count();
        assert_eq!(files, 3);
    }

    #[test]
    fn named_version_and_revert() {
        let (_d, paths, mut bus, clock) = setup();
        let bm = BackupManager::new(&paths, RotationPolicy::default(), Box::new(clock.clone()));
        bus.execute(&Command::SetConfig {
            key: "ai.model".into(),
            value: "before".into(),
        })
        .unwrap();
        let v = bm.save_version(bus.store(), "Director cut 1").unwrap();
        assert!(v.file.contains("Director cut 1"));
        clock.advance(Duration::seconds(1));
        bus.execute(&Command::SetConfig {
            key: "ai.model".into(),
            value: "after".into(),
        })
        .unwrap();
        let model = |b: &CommandBus| crate::config::ProjectConfig::from_doc(b.doc()).ai.model;
        assert_eq!(model(&bus), "after");
        let rev = bus.rev();
        let pre = bm.revert(&mut bus, &v.id).unwrap();
        assert_eq!(pre.kind, "pre-revert");
        assert_eq!(model(&bus), "before");
        assert!(bus.rev() > rev);
        // the pre-revert snapshot lets us go back
        clock.advance(Duration::seconds(1));
        bm.revert(&mut bus, &pre.id).unwrap();
        assert_eq!(model(&bus), "after");
    }
}
