//! SQLite storage for episode (`.cutaway`) and show (`.cutawayshow`) files.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::Value;

use crate::bus::Change;
use crate::error::{CoreError, Result};
use crate::model::{Document, Kind};

pub const EPISODE_SCHEMA_VERSION: i64 = 1;
pub const SHOW_SCHEMA_VERSION: i64 = 1;

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

const SHOW_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS asset (
  id TEXT PRIMARY KEY, file TEXT NOT NULL, hash TEXT NOT NULL UNIQUE, category TEXT NOT NULL DEFAULT '',
  duration_s REAL NOT NULL DEFAULT 0,
  source TEXT NOT NULL CHECK (source IN ('imported','licensed','generated')),
  license TEXT, prompt TEXT, provider TEXT, created_at TEXT NOT NULL, lufs REAL, remote_id TEXT
);
CREATE TABLE IF NOT EXISTS asset_tag (
  asset_id TEXT NOT NULL REFERENCES asset(id) ON DELETE CASCADE, tag TEXT NOT NULL,
  PRIMARY KEY (asset_id, tag)
);
CREATE TABLE IF NOT EXISTS alias (alias TEXT PRIMARY KEY, character TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS location (
  name TEXT PRIMARY KEY, room_tone_asset TEXT REFERENCES asset(id) ON DELETE SET NULL, ambience_asset TEXT
);
CREATE TABLE IF NOT EXISTS show_cue (
  id TEXT PRIMARY KEY, description TEXT NOT NULL, cue_type TEXT NOT NULL, tags TEXT NOT NULL DEFAULT '[]',
  chosen_asset TEXT REFERENCES asset(id) ON DELETE SET NULL
);
CREATE TABLE IF NOT EXISTS episode (id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS command_log (
  seq INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, client TEXT NOT NULL, name TEXT NOT NULL,
  params TEXT NOT NULL, changes TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('done','undone','discarded'))
);
"#;

fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
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
        let store = EpisodeStore {
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
        Ok(store)
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

/// Adds columns introduced after the first show schema.
fn migrate_show(conn: &Connection) -> Result<()> {
    let has_remote: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('asset') WHERE name='remote_id'")?
        .exists([])?;
    if !has_remote {
        conn.execute("ALTER TABLE asset ADD COLUMN remote_id TEXT", [])?;
    }
    Ok(())
}

/// An open show file (library database).
pub struct ShowStore {
    conn: Connection,
    path: PathBuf,
}

impl ShowStore {
    pub fn create(path: &Path) -> Result<Self> {
        if path.exists() {
            return Err(CoreError::invalid(format!(
                "{} already exists",
                path.display()
            )));
        }
        let conn = Connection::open(path)?;
        configure(&conn)?;
        conn.execute_batch(SHOW_SCHEMA)?;
        conn.execute(
            "INSERT INTO meta(key,value) VALUES('schema_version',?1),('created_at',?2),('app',?3)",
            params![
                SHOW_SCHEMA_VERSION.to_string(),
                now_iso(),
                crate::brand::DISPLAY_NAME
            ],
        )?;
        Ok(ShowStore {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(CoreError::not_found(
                "show file",
                path.display().to_string(),
            ));
        }
        let conn = Connection::open(path)?;
        configure(&conn)?;
        conn.execute_batch(SHOW_SCHEMA)?;
        migrate_show(&conn)?;
        Ok(ShowStore {
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

    /// Logs a show-level operation (show commands are applied directly; see D21).
    pub fn log(&self, client: &str, name: &str, params: &Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO command_log(ts,client,name,params,changes,state) VALUES(?1,?2,?3,?4,'[]','done')",
            params![now_iso(), client, name, serde_json::to_string(params)?],
        )?;
        Ok(())
    }

    pub fn aliases(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT alias, character FROM alias ORDER BY alias")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn add_alias(&self, alias: &str, character: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO alias(alias,character) VALUES(?1,?2) ON CONFLICT(alias) DO UPDATE SET character=excluded.character",
            params![alias, character],
        )?;
        self.log(
            "core",
            "add_alias",
            &serde_json::json!({"alias": alias, "character": character}),
        )?;
        Ok(())
    }

    pub fn register_episode(&self, id: &str, name: &str, path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO episode(id,name,path) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET name=excluded.name, path=excluded.path",
            params![id, name, path],
        )?;
        Ok(())
    }

    pub fn episodes(&self) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, path FROM episode ORDER BY name")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}
