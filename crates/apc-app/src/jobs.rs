//! Background jobs (analysis) with progress. Jobs run on a worker thread with their own
//! database connection; the open project picks up the result through the revision check.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct JobInfo {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub progress: f64,
    pub stage: String,
    pub message: String,
    pub result: Value,
}

#[derive(Clone, Default)]
pub struct Jobs(Arc<Mutex<BTreeMap<String, JobInfo>>>);

impl Jobs {
    pub fn list(&self) -> Vec<JobInfo> {
        self.0
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn update(&self, id: &str, f: impl FnOnce(&mut JobInfo)) {
        if let Ok(mut m) = self.0.lock() {
            if let Some(j) = m.get_mut(id) {
                f(j);
            }
        }
    }

    /// Starts the full analysis of `file` in the background.
    pub fn start_analysis(&self, file: PathBuf) -> String {
        let id = apc_core::ids::new_id("job");
        if let Ok(mut m) = self.0.lock() {
            m.insert(
                id.clone(),
                JobInfo {
                    id: id.clone(),
                    kind: "analysis".into(),
                    state: "running".into(),
                    progress: 0.0,
                    stage: "start".into(),
                    message: String::new(),
                    result: Value::Null,
                },
            );
        }
        let jobs = self.clone();
        let jid = id.clone();
        std::thread::spawn(move || {
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || -> crate::Result<Value> {
                    let mut svc = crate::service::Service::open(&file, "pipeline")?;
                    let j2 = jobs.clone();
                    let jid2 = jid.clone();
                    let r = svc.analyze(&move |p| {
                        j2.update(&jid2, |j| {
                            j.progress = p.fraction;
                            j.stage = p.stage.clone();
                            j.message = p.message.clone();
                        })
                    })?;
                    Ok(
                        serde_json::json!({ "takes_found": r.takes_found, "lines_matched": r.lines_matched, "picks_found": r.picks_found, "flagged": r.flagged }),
                    )
                },
            ));
            jobs.update(&jid, |j| match run {
                Ok(Ok(v)) => {
                    j.state = "done".into();
                    j.progress = 1.0;
                    j.result = v;
                }
                Ok(Err(e)) => {
                    j.state = "failed".into();
                    j.message = e.to_string();
                }
                Err(_) => {
                    j.state = "failed".into();
                    j.message = "the analysis crashed".into();
                }
            });
        });
        id
    }
}
