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

    /// Runs `work` on a worker thread with its own connection to the episode. `work` gets a
    /// progress callback (0..1, message). Panics are caught and reported as failures.
    pub fn spawn<F>(&self, kind: &str, file: PathBuf, work: F) -> String
    where
        F: FnOnce(&mut crate::service::Service, &dyn Fn(f64, &str)) -> crate::Result<Value>
            + Send
            + 'static,
    {
        let id = apc_core::ids::new_id("job");
        if let Ok(mut m) = self.0.lock() {
            m.insert(
                id.clone(),
                JobInfo {
                    id: id.clone(),
                    kind: kind.into(),
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
                    let progress = move |f: f64, msg: &str| {
                        j2.update(&jid2, |j| {
                            j.progress = f;
                            j.message = msg.to_string();
                        })
                    };
                    work(&mut svc, &progress)
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
                    j.message = "the job crashed".into();
                }
            });
        });
        id
    }

    /// Starts the full analysis of `file` in the background.
    pub fn start_analysis(
        &self,
        file: PathBuf,
        scope: Option<apc_analysis::AnalysisScope>,
    ) -> String {
        let jobs = self.clone();
        let id_cell = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let id_for_work = id_cell.clone();
        let id = self.spawn("analysis", file, move |svc, progress| {
            let jid = id_for_work.lock().map(|g| g.clone()).unwrap_or_default();
            let scope = scope.unwrap_or_else(apc_analysis::AnalysisScope::everything);
            let marks = scope.marks.len();
            let r = svc.analyze_with(scope, &|p| {
                progress(p.fraction, &p.message);
                jobs.update(&jid, |j| j.stage = p.stage.clone());
            })?;
            let rescued: usize = r.roles.iter().map(|x| x.rescued).sum();
            Ok(serde_json::json!({ "takes_found": r.takes_found, "lines_matched": r.lines_matched, "picks_found": r.picks_found, "rescued": rescued, "marks": marks, "flagged": r.flagged }))
        });
        if let Ok(mut g) = id_cell.lock() {
            *g = id.clone();
        }
        id
    }

    pub fn get(&self, id: &str) -> Option<JobInfo> {
        self.0.lock().ok().and_then(|m| m.get(id).cloned())
    }
}
