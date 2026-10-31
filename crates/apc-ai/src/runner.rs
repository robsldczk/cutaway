//! Runs AI tasks: renders the prompt, calls the provider, validates the output (one retry,
//! then flags the task), caches by input hash + prompt version + provider + model and logs
//! every call to `Logs/ai-calls.jsonl`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};

use crate::prompts::prompt;
use crate::provider::{AiError, AiProvider, AiRequest};
use crate::task::{output_schema, AiTaskIo, Validate};

/// Result of a task: the output, or a reason it was flagged for manual review.
#[derive(Debug, Clone)]
pub struct AiOutcome<T> {
    pub output: Option<T>,
    pub flagged: Option<String>,
    pub cached: bool,
}

#[derive(Clone)]
pub struct AiRunner {
    provider: Arc<dyn AiProvider>,
    cache_dir: Option<PathBuf>,
    log_path: Option<PathBuf>,
}

impl AiRunner {
    pub fn new(
        provider: Arc<dyn AiProvider>,
        cache_dir: Option<PathBuf>,
        log_path: Option<PathBuf>,
    ) -> Self {
        AiRunner {
            provider,
            cache_dir,
            log_path,
        }
    }

    pub fn provider_id(&self) -> &str {
        self.provider.id()
    }

    pub fn model(&self) -> &str {
        self.provider.model()
    }

    fn key(&self, task: &str, version: u32, input: &Value) -> String {
        let s = format!(
            "{task}|v{version}|{}|{}|{}",
            self.provider.id(),
            self.provider.model(),
            serde_json::to_string(input).unwrap_or_default()
        );
        blake3::hash(s.as_bytes()).to_hex().to_string()
    }

    fn log(&self, entry: Value) {
        let Some(p) = &self.log_path else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            let _ = writeln!(f, "{entry}");
        }
    }

    /// Runs one task. Provider failures are errors; invalid output is retried once and
    /// then flagged (`output: None`).
    pub fn run<I: AiTaskIo>(&self, input: &I) -> Result<AiOutcome<I::Output>, AiError> {
        let p = prompt(I::TASK);
        let input_v = serde_json::to_value(input)?;
        let key = self.key(I::TASK.as_str(), p.version, &input_v);
        let cache_file = self
            .cache_dir
            .as_ref()
            .map(|d| d.join(I::TASK.as_str()).join(format!("{key}.json")));
        if let Some(cf) = &cache_file {
            if let Ok(s) = std::fs::read_to_string(cf) {
                if let Ok(out) = serde_json::from_str::<I::Output>(&s) {
                    return Ok(AiOutcome {
                        output: Some(out),
                        flagged: None,
                        cached: true,
                    });
                }
            }
        }
        let req = AiRequest {
            task: I::TASK,
            prompt: p.text.to_string(),
            prompt_version: p.version,
            input: input_v,
            output_schema: output_schema::<I>(),
        };
        let mut last_err = String::new();
        for attempt in 0..2 {
            let start = Instant::now();
            let raw = self.provider.run(&req);
            let ms = start.elapsed().as_millis() as u64;
            let raw = match raw {
                Ok(v) => v,
                Err(e) => {
                    self.log(json!({
                        "ts": chrono::Utc::now().to_rfc3339(), "task": I::TASK.as_str(),
                        "provider": self.provider.id(), "model": self.provider.model(),
                        "prompt_version": p.version, "input_hash": key, "duration_ms": ms,
                        "ok": false, "error": e.to_string(), "attempt": attempt,
                    }));
                    return Err(e);
                }
            };
            let parsed = serde_json::from_value::<I::Output>(raw.clone())
                .map_err(|e| e.to_string())
                .and_then(|o| o.validate().map(|_| o));
            self.log(json!({
                "ts": chrono::Utc::now().to_rfc3339(), "task": I::TASK.as_str(),
                "provider": self.provider.id(), "model": self.provider.model(),
                "prompt_version": p.version, "input_hash": key, "duration_ms": ms,
                "ok": parsed.is_ok(), "attempt": attempt, "output": raw,
            }));
            match parsed {
                Ok(out) => {
                    if let Some(cf) = &cache_file {
                        if let Some(dir) = cf.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(cf, serde_json::to_string(&out)?);
                    }
                    return Ok(AiOutcome {
                        output: Some(out),
                        flagged: None,
                        cached: false,
                    });
                }
                Err(e) => last_err = e,
            }
        }
        Ok(AiOutcome {
            output: None,
            flagged: Some(format!(
                "{} output invalid twice: {last_err}",
                I::TASK.as_str()
            )),
            cached: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockProvider;
    use crate::task::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn picks_input() -> PickDetectInput {
        PickDetectInput {
            takes: vec![PickCandidate {
                take_id: "t1".into(),
                take_no: 1,
                line_text: "Lunch.".into(),
                feedback_text: "that one was the right one".into(),
            }],
        }
    }

    #[test]
    fn caches_and_logs() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("Logs/ai-calls.jsonl");
        let r = AiRunner::new(
            Arc::new(MockProvider::new()),
            Some(dir.path().join("Cache/ai")),
            Some(log.clone()),
        );
        let a = r.run(&picks_input()).unwrap();
        assert!(!a.cached);
        assert_eq!(a.output.unwrap().picks[0].strength, 2);
        let b = r.run(&picks_input()).unwrap();
        assert!(b.cached);
        let lines = std::fs::read_to_string(&log).unwrap();
        assert_eq!(lines.lines().count(), 1);
        let entry: Value = serde_json::from_str(lines.lines().next().unwrap()).unwrap();
        assert_eq!(entry["task"], "pick_detect");
        assert_eq!(entry["provider"], "mock");
    }

    struct Bad(AtomicUsize);
    impl AiProvider for Bad {
        fn id(&self) -> &str {
            "bad"
        }
        fn model(&self) -> &str {
            "bad"
        }
        fn run(&self, _r: &AiRequest) -> Result<Value, AiError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"picks": [{"take_id": "t1", "strength": 7, "refers_to": null}]}))
        }
    }

    #[test]
    fn invalid_output_retries_once_then_flags() {
        let bad = Arc::new(Bad(AtomicUsize::new(0)));
        let r = AiRunner::new(bad.clone(), None, None);
        let out = r.run(&picks_input()).unwrap();
        assert!(out.output.is_none());
        assert!(out.flagged.unwrap().contains("strength"));
        assert_eq!(bad.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn schemas_are_objects() {
        let s = output_schema::<PickDetectInput>();
        assert!(s.to_string().contains("strength"));
    }
}
