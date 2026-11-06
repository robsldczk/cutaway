//! SQLite storage for episode (`.cutaway`) files. (A show file is JSON, see `show`.)

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::Value;

use crate::bus::Change;
use crate::error::{CoreError, Result};
use crate::model::{Document, Kind};

pub const EPISODE_SCHEMA_VERSION: i64 = 2;

const EPISODE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS entity (
  kind   TEXT NOT NULL,
  id     TEXT NOT NULL,
  parent TEXT,
  ord    REAL NOT NULL DEFAULT 0,
  data   TEXT NOT NULL,
  PRIMARY KEY (kind, id)
);
CREATE INDEX IF NOT EXISTS entity_parent ON entity(kind, parent, ord);
CREATE TABLE IF NOT EXISTS command_log (
  seq     INTEGER PRIMARY KEY AUTOINCREMENT,
  ts      TEXT NOT NULL,
  client  TEXT NOT NULL,
  name    TEXT NOT NULL,
  params  TEXT NOT NULL,
  changes TEXT NOT NULL,
  state   TEXT NOT NULL CHECK (state IN ('done','undone','discarded'))
);
CREATE TABLE IF NOT EXISTS jobs (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, state TEXT NOT NULL, progress REAL NOT NULL DEFAULT 0,
  message TEXT, started_at TEXT, finished_at TEXT, result TEXT
);
CREATE TABLE IF NOT EXISTS backups (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('auto','named','pre-revert')),
  name TEXT, file TEXT NOT NULL, created_at TEXT NOT NULL, seq INTEGER
);
CREATE VIEW IF NOT EXISTS v_scenes AS SELECT id, json_extract(data,'$.number') AS number,
  json_extract(data,'$.heading') AS heading, json_extract(data,'$.location') AS location
  FROM entity WHERE kind='scene';
CREATE VIEW IF NOT EXISTS v_lines AS SELECT id, parent AS scene_id, ord AS number,
  json_extract(data,'$.kind') AS line_kind, json_extract(data,'$.character') AS character,
  json_extract(data,'$.text') AS text FROM entity WHERE kind='line';
CREATE VIEW IF NOT EXISTS v_sources AS SELECT id, json_extract(data,'$.path') AS path,
  json_extract(data,'$.role') AS role, json_extract(data,'$.character') AS character,
  json_extract(data,'$.hash') AS hash, json_extract(data,'$.offset_s') AS offset_s
  FROM entity WHERE kind='source';
CREATE VIEW IF NOT EXISTS v_takes AS SELECT id, parent AS line_id, ord AS take_no,
  json_extract(data,'$.source_id') AS source_id, json_extract(data,'$.take_type') AS take_type,
  json_extract(data,'$.t0') AS t0, json_extract(data,'$.t1') AS t1,
  json_extract(data,'$.pick') AS pick, json_extract(data,'$.favorite') AS favorite
  FROM entity WHERE kind='take';
CREATE VIEW IF NOT EXISTS v_boxes AS SELECT id AS line_id, json_extract(data,'$.take_id') AS take_id,
  json_extract(data,'$.gap_s') AS gap_s, json_extract(data,'$.head_s') AS head_s,
  json_extract(data,'$.tail_s') AS tail_s FROM entity WHERE kind='box';
CREATE VIEW IF NOT EXISTS v_notes AS SELECT id, json_extract(data,'$.class') AS class,
  json_extract(data,'$.t0') AS t0, json_extract(data,'$.text') AS text FROM entity WHERE kind='note';
"#;

/// Opens an episode file read-only without leaving anything next to it. A closed episode
/// (no write-ahead log) opens immutable, so SQLite creates no `-wal`/`-shm` files; one
/// that is open somewhere has its log read along.
fn open_untouched(path: &Path) -> Result<Connection> {
    use rusqlite::OpenFlags;
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    let live = std::fs::metadata(&wal)
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    if live {
        return Ok(Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?);
    }
    let mut uri = String::from("file:");
    for c in path.to_string_lossy().replace('\\', "/").chars() {
        match c {
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3F"),
            '#' => uri.push_str("%23"),
            c => uri.push(c),
        }
    }
    uri.push_str("?immutable=1");
    Ok(Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    Ok(())
}

/// Adds the fields a row of `kind` doesn't have yet. Existing values are left alone.
fn fill_missing(conn: &Connection, kind: Kind, fields: &[(&str, Value)]) -> Result<()> {
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT id, data FROM entity WHERE kind=?1")?
        .query_map([kind.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, data) in rows {
        let mut v: Value = serde_json::from_str(&data)?;
        let Some(obj) = v.as_object_mut() else {
            continue;
        };
        for (key, value) in fields {
            obj.entry(key.to_string()).or_insert_with(|| value.clone());
        }
        conn.execute(
            "UPDATE entity SET data=?1 WHERE kind=?2 AND id=?3",
            params![serde_json::to_string(&v)?, kind.as_str(), id],
        )?;
    }
    Ok(())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// One row of the command log.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogRow {
    pub seq: i64,
    pub ts: String,
    pub client: String,
    pub name: String,
    pub params: Value,
    pub state: String,
}

/// An open episode file.
pub struct EpisodeStore {
    conn: Connection,
    path: PathBuf,
}

impl EpisodeStore {
    /// Creates a new episode file. Fails if the file exists.
    pub fn create(path: &Path) -> Result<Self> {
        if path.exists() {
            return Err(CoreError::invalid(format!(
                "{} already exists",
                path.display()
            )));
        }
        let conn = Connection::open(path)?;
        configure(&conn)?;
        conn.execute_batch(EPISODE_SCHEMA)?;
        let store = EpisodeStore {
            conn,
            path: path.to_path_buf(),
        };
        store.set_meta("schema_version", &EPISODE_SCHEMA_VERSION.to_string())?;
        store.set_meta("created_at", &now_iso())?;
        store.set_meta("app", crate::brand::DISPLAY_NAME)?;
        store.set_meta("rev", "0")?;
        Ok(store)
    }

    /// Opens an existing episode file.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(CoreError::not_found(
                "episode file",
                path.display().to_string(),
            ));
        }
        let conn = Connection::open(path)?;
        configure(&conn)?;
        conn.execute_batch(EPISODE_SCHEMA)?;
        let mut store = EpisodeStore {
            conn,
            path: path.to_path_buf(),
        };
        let v: i64 = store
            .meta("schema_version")?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if v > EPISODE_SCHEMA_VERSION {
            return Err(CoreError::SchemaVersion {
                found: v,
                supported: EPISODE_SCHEMA_VERSION,
            });
        }
        if v < EPISODE_SCHEMA_VERSION {
            store.migrate()?;
        }
        Ok(store)
    }

    /// Brings an older file up to date, all or nothing. Values are written into the rows, so
    /// retuning a default later doesn't change the sound of a finished scene. The document
    /// means the same afterwards, so `rev` stays and no undo step is made.
    fn migrate(&mut self) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // another process may have migrated while this one waited for the lock
        let v: i64 = tx
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if v < 2 {
            let room = serde_json::to_value(crate::model::SceneRoom::default())?;
            fill_missing(&tx, Kind::Scene, &[("room", room)])?;
            fill_missing(
                &tx,
                Kind::Placement,
                &[
                    ("pan", Value::from(0.0)),
                    ("depth", Value::from(0.0)),
                    // their fades were set by hand or by an older default: they stay
                    ("auto_fade", Value::from(false)),
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO meta(key,value) VALUES('schema_version',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [EPISODE_SCHEMA_VERSION.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Opens an episode file only to read it (nothing is written, not even the log files'
    /// headers): for copying another episode.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(CoreError::not_found(
                "episode file",
                path.display().to_string(),
            ));
        }
        let conn = open_untouched(path)?;
        Ok(EpisodeStore {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Folds the write-ahead log into the episode file, so a sync tool that copies the
    /// file alone (Syncthing) carries every change. Other readers keep working.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    /// A consistent single-file copy of the episode (write-ahead log included).
    pub fn copy_to(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            return Err(CoreError::invalid(format!(
                "{} already exists",
                dest.display()
            )));
        }
        self.conn
            .execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
        Ok(())
    }

    /// Copies an episode file without writing to it (opened read-only; its write-ahead log
    /// is included).
    pub fn copy_file(src: &Path, dest: &Path) -> Result<()> {
        if dest.exists() {
            return Err(CoreError::invalid(format!(
                "{} already exists",
                dest.display()
            )));
        }
        let conn = open_untouched(src)?;
        conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
        Ok(())
    }

    /// Document revision; bumps on every write from any process.
    pub fn rev(&self) -> Result<i64> {
        Ok(self.meta("rev")?.and_then(|s| s.parse().ok()).unwrap_or(0))
    }

    pub fn load_document(&self) -> Result<Document> {
        let mut doc = Document::default();
        let mut stmt = self.conn.prepare("SELECT kind, id, data FROM entity")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (kind, id, data) = row?;
            let kind = Kind::parse(&kind)?;
            let v: Value = serde_json::from_str(&data)?;
            doc.put_value(kind, &id, Some(&v))?;
        }
        doc.drop_unused_cuts();
        Ok(doc)
    }

    /// Writes a new command: discards the redo tail, logs it, writes its rows, bumps rev.
    /// Fails with `Conflict` when another process wrote since `expected_rev`.
    pub(crate) fn commit_command(
        &mut self,
        expected_rev: i64,
        client: &str,
        name: &str,
        params: &Value,
        changes: &[Change],
    ) -> Result<CommitOutcome> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rev: i64 = tx
            .query_row("SELECT value FROM meta WHERE key='rev'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if rev != expected_rev {
            return Ok(CommitOutcome::Conflict);
        }
        tx.execute(
            "UPDATE command_log SET state='discarded' WHERE state='undone'",
            [],
        )?;
        tx.execute(
            "INSERT INTO command_log(ts,client,name,params,changes,state) VALUES(?1,?2,?3,?4,?5,'done')",
            params![
                now_iso(),
                client,
                name,
                serde_json::to_string(params)?,
                serde_json::to_string(changes)?
            ],
        )?;
        let seq = tx.last_insert_rowid();
        for c in changes {
            write_row(
                &tx,
                c.kind,
                &c.id,
                c.after.as_ref(),
                c.parent.as_deref(),
                c.ord,
            )?;
        }
        let new_rev = rev + 1;
        tx.execute(
            "INSERT INTO meta(key,value) VALUES('rev',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [new_rev.to_string()],
        )?;
        tx.commit()?;
        Ok(CommitOutcome::Done { seq, rev: new_rev })
    }

    /// Loads the changes of a log entry.
    pub(crate) fn log_changes(&self, seq: i64) -> Result<Vec<Change>> {
        let s: String =
            self.conn
                .query_row("SELECT changes FROM command_log WHERE seq=?1", [seq], |r| {
                    r.get(0)
                })?;
        Ok(serde_json::from_str(&s)?)
    }

    /// The newest `done` entry (undo target) or the oldest `undone` one (redo target).
    pub(crate) fn undo_target(&self) -> Result<Option<(i64, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, name FROM command_log WHERE state='done' ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub(crate) fn redo_target(&self) -> Result<Option<(i64, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, name FROM command_log WHERE state='undone' ORDER BY seq ASC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Applies the before (undo) or after (redo) side of a logged command.
    pub(crate) fn apply_log_entry(
        &mut self,
        expected_rev: i64,
        seq: i64,
        undo: bool,
    ) -> Result<CommitOutcome> {
        let changes = self.log_changes(seq)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rev: i64 = tx
            .query_row("SELECT value FROM meta WHERE key='rev'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if rev != expected_rev {
            return Ok(CommitOutcome::Conflict);
        }
        if undo {
            for c in changes.iter().rev() {
                write_row(
                    &tx,
                    c.kind,
                    &c.id,
                    c.before.as_ref(),
                    c.before_parent.as_deref(),
                    c.before_ord,
                )?;
            }
        } else {
            for c in &changes {
                write_row(
                    &tx,
                    c.kind,
                    &c.id,
                    c.after.as_ref(),
                    c.parent.as_deref(),
                    c.ord,
                )?;
            }
        }
        tx.execute(
            "UPDATE command_log SET state=?1 WHERE seq=?2",
            params![if undo { "undone" } else { "done" }, seq],
        )?;
        let new_rev = rev + 1;
        tx.execute(
            "INSERT INTO meta(key,value) VALUES('rev',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [new_rev.to_string()],
        )?;
        tx.commit()?;
        Ok(CommitOutcome::Done { seq, rev: new_rev })
    }

    pub fn history(&self, limit: usize) -> Result<Vec<LogRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, ts, client, name, params, state FROM command_log WHERE state != 'discarded' ORDER BY seq DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok(LogRow {
                seq: r.get(0)?,
                ts: r.get(1)?,
                client: r.get(2)?,
                name: r.get(3)?,
                params: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or(Value::Null),
                state: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Bumps the revision after an out-of-band change (e.g. a revert).
    pub(crate) fn bump_rev(&self) -> Result<i64> {
        let rev = self.rev()? + 1;
        self.set_meta("rev", &rev.to_string())?;
        Ok(rev)
    }
}

pub(crate) enum CommitOutcome {
    Done { seq: i64, rev: i64 },
    Conflict,
}

fn write_row(
    conn: &Connection,
    kind: Kind,
    id: &str,
    value: Option<&Value>,
    parent: Option<&str>,
    ord: f64,
) -> Result<()> {
    match value {
        Some(v) => {
            conn.execute(
                "INSERT INTO entity(kind,id,parent,ord,data) VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(kind,id) DO UPDATE SET parent=excluded.parent, ord=excluded.ord, data=excluded.data",
                params![kind.as_str(), id, parent, ord, serde_json::to_string(v)?],
            )?;
        }
        None => {
            conn.execute(
                "DELETE FROM entity WHERE kind=?1 AND id=?2",
                params![kind.as_str(), id],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Lane, SceneRoom};
    use serde_json::json;

    /// An episode file as version 1 wrote it: rows without the room or the sound fields.
    fn v1_file(dir: &Path) -> PathBuf {
        let path = dir.join("old.cutaway");
        let store = EpisodeStore::create(&path).unwrap();
        store.set_meta("schema_version", "1").unwrap();
        store.set_meta("rev", "7").unwrap();
        let rows = [
            (
                "scene",
                "sc1",
                json!({ "id": "sc1", "number": 1, "heading": "INT. LAB", "tightness": 0.5 }),
            ),
            (
                "scene",
                "sc2",
                json!({ "id": "sc2", "number": 2, "heading": "INT. HALL" }),
            ),
            (
                "placement",
                "p1",
                json!({
                    "id": "p1", "scene_id": "sc1", "asset_id": "a", "lane": "sfx", "len_s": 2.5,
                    "fade_in_s": 0.01, "fade_out_s": 0.05, "gain_db": -3.0,
                }),
            ),
        ];
        for (kind, id, data) in rows {
            store
                .conn()
                .execute(
                    "INSERT INTO entity(kind,id,ord,data) VALUES(?1,?2,0,?3)",
                    params![kind, id, data.to_string()],
                )
                .unwrap();
        }
        path
    }

    fn row(store: &EpisodeStore, kind: &str, id: &str) -> Value {
        let data: String = store
            .conn()
            .query_row(
                "SELECT data FROM entity WHERE kind=?1 AND id=?2",
                [kind, id],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&data).unwrap()
    }

    #[test]
    fn opening_a_version_1_file_gives_every_scene_a_room() {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::open(&v1_file(dir.path())).unwrap();
        assert_eq!(store.meta("schema_version").unwrap().as_deref(), Some("2"));
        let doc = store.load_document().unwrap();
        assert_eq!(doc.scenes["sc1"].room, SceneRoom::default());
        assert_eq!(doc.scenes["sc2"].room, SceneRoom::default());
        assert_eq!(doc.scenes["sc1"].tightness, 0.5, "the rest stays");
        // written into the row, so retuning the default later can't change the scene
        assert_eq!(
            row(&store, "scene", "sc1")["room"],
            serde_json::to_value(SceneRoom::default()).unwrap()
        );
    }

    #[test]
    fn migrated_sounds_keep_their_fades_and_level() {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::open(&v1_file(dir.path())).unwrap();
        let doc = store.load_document().unwrap();
        let p = &doc.placements["p1"];
        assert_eq!((p.pan, p.depth), (0.0, 0.0));
        assert!(!p.auto_fade, "fades set before auto existed stay as set");
        assert_eq!((p.fade_in_s, p.fade_out_s), (0.01, 0.05));
        assert_eq!((p.gain_db, p.len_s), (-3.0, 2.5));
        assert_eq!(row(&store, "placement", "p1")["depth"], json!(0.0));
    }

    #[test]
    fn migrating_is_not_an_edit() {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::open(&v1_file(dir.path())).unwrap();
        assert_eq!(store.rev().unwrap(), 7);
        assert_eq!(store.history(10).unwrap().len(), 0, "no undo step");
    }

    #[test]
    fn migrating_runs_once_and_keeps_what_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = v1_file(dir.path());
        drop(EpisodeStore::open(&path).unwrap());
        let room = json!({ "size": 0.9, "damping": 0.1, "wet": 0.2 });
        {
            let store = EpisodeStore::open(&path).unwrap();
            let mut v = row(&store, "scene", "sc1");
            v["room"] = room.clone();
            store
                .conn()
                .execute(
                    "UPDATE entity SET data=?1 WHERE kind='scene' AND id='sc1'",
                    [v.to_string()],
                )
                .unwrap();
            // and a row that already has the field survives a migration too
            store.set_meta("schema_version", "1").unwrap();
        }
        let store = EpisodeStore::open(&path).unwrap();
        assert_eq!(row(&store, "scene", "sc1")["room"], room);
    }

    #[test]
    fn a_file_from_a_newer_version_is_refused_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.cutaway");
        let store = EpisodeStore::create(&path).unwrap();
        store.set_meta("schema_version", "99").unwrap();
        drop(store);
        assert!(matches!(
            EpisodeStore::open(&path),
            Err(CoreError::SchemaVersion { found: 99, .. })
        ));
    }

    #[test]
    fn a_new_file_needs_no_migration() {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::create(&dir.path().join("n.cutaway")).unwrap();
        assert_eq!(
            store.meta("schema_version").unwrap().as_deref(),
            Some(EPISODE_SCHEMA_VERSION.to_string().as_str())
        );
    }

    /// What the vocal buckets left in a file: (kind, id, row).
    fn vocals_left_behind() -> Vec<(&'static str, &'static str, Value)> {
        let cut = |id: &str| {
            json!({ "id": id, "file": format!("Vocals/Paul/{id}.wav"), "hash": id,
                    "category": "Vocals/Paul", "source": "recorded" })
        };
        vec![
            (
                "bucket",
                "bk_vocals_paul",
                json!({ "id": "bk_vocals_paul", "name": "Paul · vocals", "character": "PAUL",
                        "keywords": [], "scene_ids": [], "cue_ids": [], "kept": ["a_idle", "a_used"],
                        "previews": [], "dismissed": [], "exclude": [] }),
            ),
            ("asset", "a_idle", cut("a_idle")),
            ("asset", "a_used", cut("a_used")),
            (
                "asset",
                "a_door",
                json!({ "id": "a_door", "file": "SFX/door.wav", "hash": "door", "source": "imported" }),
            ),
            (
                "placement",
                "p1",
                json!({ "id": "p1", "scene_id": "sc1", "asset_id": "a_used", "lane": "adlib",
                        "character": "PAUL", "layer": 1, "at_s": 1.0, "len_s": 0.5,
                        "fade_in_s": 0.01, "fade_out_s": 0.05 }),
            ),
        ]
    }

    #[test]
    fn an_older_file_reads_without_its_vocals_and_keeps_them_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.cutaway");
        let store = EpisodeStore::create(&path).unwrap();
        for (kind, id, row) in vocals_left_behind() {
            store
                .conn()
                .execute(
                    "INSERT INTO entity(kind,id,data) VALUES(?1,?2,?3)",
                    params![kind, id, row.to_string()],
                )
                .unwrap();
        }
        let doc = store.load_document().unwrap();
        assert!(doc.buckets.is_empty());
        // a cut nobody uses is gone from the document, a placed one stays, others aren't touched
        let assets: Vec<&str> = doc.assets.keys().map(String::as_str).collect();
        assert_eq!(assets, ["a_door", "a_used"]);
        // the adlib track is gone: the sound sits on its character's lane, out of the room
        let p = &doc.placements["p1"];
        assert_eq!(p.lane, Lane::Dialogue);
        assert_eq!(p.character.as_deref(), Some("PAUL"));
        assert!(!p.in_room());
        // and the file is as it was: reading changes nothing
        assert_eq!(row(&store, "bucket", "bk_vocals_paul")["character"], "PAUL");
        assert_eq!(row(&store, "asset", "a_idle")["id"], "a_idle");
        assert_eq!(row(&store, "placement", "p1")["lane"], "adlib");
    }

    #[test]
    fn undo_or_a_backup_cant_bring_a_vocal_bucket_back() {
        let mut doc = Document::default();
        let (_, id, row) = vocals_left_behind().remove(0);
        doc.put_value(Kind::Bucket, id, Some(&row)).unwrap();
        assert!(doc.buckets.is_empty());
    }

    #[test]
    fn a_read_only_copy_of_an_old_file_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let store = EpisodeStore::open_read_only(&v1_file(dir.path())).unwrap();
        let doc = store.load_document().unwrap();
        assert_eq!(doc.scenes["sc2"].room, SceneRoom::default());
    }
}
