//! The command bus: every document change is a command, persisted immediately,
//! undoable from any client (UI, CLI, MCP).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::Command;
use crate::error::{CoreError, Result};
use crate::model::{Document, Entity, Kind};
use crate::store::{CommitOutcome, EpisodeStore, LogRow};
use crate::timeline::{broken_lock, locked_timing};

/// The command that may change a locked scene.
const SCRIPT_IMPORT: &str = "import_script_version";

/// One entity change with both sides, so undo and redo are generic.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Change {
    pub kind: Kind,
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub ord: f64,
    pub before: Option<Value>,
    pub after: Option<Value>,
    #[serde(default)]
    pub before_parent: Option<String>,
    #[serde(default)]
    pub before_ord: f64,
}

/// A command in progress. Writes go to the document immediately and are recorded;
/// on error the bus rolls them back.
pub struct Tx<'a> {
    doc: &'a mut Document,
    changes: Vec<Change>,
}

impl<'a> Tx<'a> {
    fn new(doc: &'a mut Document) -> Self {
        Tx {
            doc,
            changes: Vec::new(),
        }
    }

    pub fn doc(&self) -> &Document {
        self.doc
    }

    /// Inserts or replaces an entity. Unchanged values are not recorded.
    pub fn put<T: Entity>(&mut self, e: &T) -> Result<()> {
        let id = e.id().to_string();
        let after = serde_json::to_value(e)?;
        let before = self.doc.get_value(T::KIND, &id)?;
        if before.as_ref() == Some(&after) {
            return Ok(());
        }
        let (before_parent, before_ord) = match &before {
            Some(v) => {
                let old: T = serde_json::from_value(v.clone())?;
                (old.parent().map(str::to_string), old.ord())
            }
            None => (None, 0.0),
        };
        self.doc.put_value(T::KIND, &id, Some(&after))?;
        self.changes.push(Change {
            kind: T::KIND,
            id,
            parent: e.parent().map(str::to_string),
            ord: e.ord(),
            before,
            after: Some(after),
            before_parent,
            before_ord,
        });
        Ok(())
    }

    /// Removes an entity if present.
    pub fn delete<T: Entity>(&mut self, id: &str) -> Result<()> {
        let before = self.doc.get_value(T::KIND, id)?;
        let Some(before) = before else {
            return Ok(());
        };
        let old: T = serde_json::from_value(before.clone())?;
        self.doc.put_value(T::KIND, id, None)?;
        self.changes.push(Change {
            kind: T::KIND,
            id: id.to_string(),
            parent: None,
            ord: 0.0,
            before: Some(before),
            after: None,
            before_parent: old.parent().map(str::to_string),
            before_ord: old.ord(),
        });
        Ok(())
    }

    fn rollback(self) -> Result<()> {
        for c in self.changes.iter().rev() {
            self.doc.put_value(c.kind, &c.id, c.before.as_ref())?;
        }
        Ok(())
    }

    fn into_changes(self) -> Vec<Change> {
        self.changes
    }
}

/// Result of executing a command.
#[derive(Debug, Clone, Serialize)]
pub struct Executed {
    /// Log sequence number, `None` when the command changed nothing.
    pub seq: Option<i64>,
    pub name: String,
    pub changes: usize,
    pub result: Value,
}

/// Result of an undo or redo.
#[derive(Debug, Clone, Serialize)]
pub struct Stepped {
    pub seq: i64,
    pub name: String,
}

/// Called after every successful write (autosave hook).
pub type WriteHook = Box<dyn FnMut(&EpisodeStore) + Send>;

/// Owns the episode store and the in-memory document.
pub struct CommandBus {
    store: EpisodeStore,
    doc: Document,
    rev: i64,
    client: String,
    /// Called after every successful write (autosave hook).
    on_write: Option<WriteHook>,
}

impl CommandBus {
    pub fn new(store: EpisodeStore, client: impl Into<String>) -> Result<Self> {
        let doc = store.load_document()?;
        let rev = store.rev()?;
        Ok(CommandBus {
            store,
            doc,
            rev,
            client: client.into(),
            on_write: None,
        })
    }

    pub fn set_on_write(&mut self, f: WriteHook) {
        self.on_write = Some(f);
    }

    pub fn doc(&self) -> &Document {
        &self.doc
    }

    pub fn store(&self) -> &EpisodeStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut EpisodeStore {
        &mut self.store
    }

    pub fn rev(&self) -> i64 {
        self.rev
    }

    pub fn client(&self) -> &str {
        &self.client
    }

    /// Reloads the document if another process wrote. Returns true when it did.
    pub fn refresh(&mut self) -> Result<bool> {
        let rev = self.store.rev()?;
        if rev == self.rev {
            return Ok(false);
        }
        self.doc = self.store.load_document()?;
        self.rev = rev;
        Ok(true)
    }

    /// Forces a reload (after a revert replaced the content).
    pub fn reload(&mut self) -> Result<()> {
        self.doc = self.store.load_document()?;
        self.rev = self.store.rev()?;
        Ok(())
    }

    /// Executes a command and persists it.
    pub fn execute(&mut self, cmd: &Command) -> Result<Executed> {
        let name = cmd.name().to_string();
        let params = cmd.log_params();
        self.execute_fn(&name, &params, |tx| cmd.apply(tx))
    }

    /// Executes an ad-hoc change function as a named command (used by pipeline imports).
    pub fn execute_fn(
        &mut self,
        name: &str,
        params: &Value,
        f: impl Fn(&mut Tx) -> Result<Value>,
    ) -> Result<Executed> {
        for _attempt in 0..3 {
            self.refresh()?;
            // a new script version may change a locked scene: it flags it instead (D108)
            let locks = if name == SCRIPT_IMPORT {
                Vec::new()
            } else {
                locked_timing(&self.doc)
            };
            let mut tx = Tx::new(&mut self.doc);
            let result = match f(&mut tx) {
                Ok(v) => v,
                Err(e) => {
                    tx.rollback()?;
                    return Err(e);
                }
            };
            if let Some(refusal) = broken_lock(tx.doc(), &locks) {
                tx.rollback()?;
                return Err(CoreError::SceneLocked(refusal));
            }
            let changes = tx.into_changes();
            if changes.is_empty() {
                return Ok(Executed {
                    seq: None,
                    name: name.to_string(),
                    changes: 0,
                    result,
                });
            }
            match self
                .store
                .commit_command(self.rev, &self.client, name, params, &changes)
            {
                Ok(CommitOutcome::Done { seq, rev }) => {
                    self.rev = rev;
                    if let Some(cb) = self.on_write.as_mut() {
                        cb(&self.store);
                    }
                    return Ok(Executed {
                        seq: Some(seq),
                        name: name.to_string(),
                        changes: changes.len(),
                        result,
                    });
                }
                Ok(CommitOutcome::Conflict) => {
                    revert_changes(&mut self.doc, &changes)?;
                    continue;
                }
                Err(e) => {
                    revert_changes(&mut self.doc, &changes)?;
                    return Err(e);
                }
            }
        }
        Err(CoreError::Other(
            "the project kept changing from another client; try again".into(),
        ))
    }

    pub fn can_undo(&self) -> Result<bool> {
        Ok(self.store.undo_target()?.is_some())
    }

    pub fn can_redo(&self) -> Result<bool> {
        Ok(self.store.redo_target()?.is_some())
    }

    pub fn undo(&mut self) -> Result<Option<Stepped>> {
        self.step(true)
    }

    pub fn redo(&mut self) -> Result<Option<Stepped>> {
        self.step(false)
    }

    fn step(&mut self, undo: bool) -> Result<Option<Stepped>> {
        for _attempt in 0..3 {
            self.refresh()?;
            let target = if undo {
                self.store.undo_target()?
            } else {
                self.store.redo_target()?
            };
            let Some((seq, name)) = target else {
                return Ok(None);
            };
            match self.store.apply_log_entry(self.rev, seq, undo)? {
                CommitOutcome::Done { rev, .. } => {
                    let changes = self.store.log_changes(seq)?;
                    if undo {
                        for c in changes.iter().rev() {
                            self.doc.put_value(c.kind, &c.id, c.before.as_ref())?;
                        }
                    } else {
                        for c in &changes {
                            self.doc.put_value(c.kind, &c.id, c.after.as_ref())?;
                        }
                    }
                    self.rev = rev;
                    if let Some(cb) = self.on_write.as_mut() {
                        cb(&self.store);
                    }
                    return Ok(Some(Stepped { seq, name }));
                }
                CommitOutcome::Conflict => continue,
            }
        }
        Err(CoreError::Other(
            "the project kept changing from another client; try again".into(),
        ))
    }

    pub fn history(&self, limit: usize) -> Result<Vec<LogRow>> {
        self.store.history(limit)
    }
}

fn revert_changes(doc: &mut Document, changes: &[Change]) -> Result<()> {
    for c in changes.iter().rev() {
        doc.put_value(c.kind, &c.id, c.before.as_ref())?;
    }
    Ok(())
}
