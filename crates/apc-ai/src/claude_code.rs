//! AI provider that runs the Claude Code CLI headless: `claude -p --output-format json`.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::provider::{AiError, AiProvider, AiRequest};

pub struct ClaudeCodeProvider {
    pub binary: String,
    pub model: String,
    pub effort: String,
    pub timeout: Duration,
    /// Extra CLI arguments (from `CUTAWAY_CLAUDE_ARGS`, space separated).
    pub extra_args: Vec<String>,
}

impl ClaudeCodeProvider {
    pub fn new(model: &str, effort: &str, timeout_s: u64) -> Self {
        ClaudeCodeProvider {
            binary: std::env::var("CUTAWAY_CLAUDE_BIN").unwrap_or_else(|_| "claude".into()),
            model: model.to_string(),
            effort: effort.to_string(),
            timeout: Duration::from_secs(timeout_s),
            extra_args: std::env::var("CUTAWAY_CLAUDE_ARGS")
                .map(|s| s.split_whitespace().map(String::from).collect())
                .unwrap_or_default(),
        }
    }

    /// True when the CLI can be started.
    pub fn available(&self) -> bool {
        Command::new(&self.binary)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Pulls the first JSON object out of a model reply (tolerates code fences and prose).
pub fn extract_json(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        if v.is_object() {
            return Some(v);
        }
    }
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text[start..].char_indices() {
        if in_str {
            match c {
                '\\' if !esc => esc = true,
                '"' if !esc => in_str = false,
                _ => esc = false,
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&text[start..start + i + 1]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

impl AiProvider for ClaudeCodeProvider {
    fn id(&self) -> &str {
        "claude-code"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn run(&self, req: &AiRequest) -> Result<Value, AiError> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("json")
            .arg("--model")
            .arg(&self.model)
            .args(&self.extra_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Effort is passed through the environment; see DECISIONS.md D24.
        cmd.env("CUTAWAY_AI_EFFORT", &self.effort);
        let mut child = cmd
            .spawn()
            .map_err(|e| AiError::Unavailable(format!("cannot start '{}': {e}", self.binary)))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(req.render().as_bytes())?;
        }
        let start = Instant::now();
        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if start.elapsed() > self.timeout {
                let _ = child.kill();
                return Err(AiError::Timeout(self.timeout.as_secs()));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut out = String::new();
        if let Some(mut s) = child.stdout.take() {
            s.read_to_string(&mut out)?;
        }
        let status = child.wait()?;
        if !status.success() {
            let mut err = String::new();
            if let Some(mut s) = child.stderr.take() {
                let _ = s.read_to_string(&mut err);
            }
            return Err(AiError::Failed(format!("exit {status}: {}", err.trim())));
        }
        let envelope: Value = serde_json::from_str(out.trim())
            .map_err(|e| AiError::InvalidOutput(format!("CLI output is not JSON: {e}")))?;
        if envelope.get("is_error").and_then(Value::as_bool) == Some(true) {
            return Err(AiError::Failed(
                envelope
                    .get("result")
                    .map(|r| r.to_string())
                    .unwrap_or_default(),
            ));
        }
        let text = envelope
            .get("result")
            .and_then(Value::as_str)
            .ok_or_else(|| AiError::InvalidOutput("no 'result' in CLI output".into()))?;
        extract_json(text).ok_or_else(|| AiError::InvalidOutput("no JSON object in reply".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_fenced_reply() {
        let t = "Sure.\n```json\n{\"picks\": [{\"take_id\": \"a\", \"strength\": 2, \"refers_to\": null}]}\n```";
        let v = extract_json(t).unwrap();
        assert_eq!(v["picks"][0]["strength"], 2);
        assert!(extract_json("no json here").is_none());
        let nested = extract_json("x {\"a\": {\"b\": \"}\"}} y").unwrap();
        assert_eq!(nested["a"]["b"], "}");
    }

    /// Live call through the Claude Code CLI (needs `claude` and a login).
    #[test]
    #[ignore = "live Claude Code call; run with --ignored when the CLI is installed"]
    fn live_pick_detection() {
        use crate::task::*;
        let p = ClaudeCodeProvider::new("claude-sonnet-5-5", "medium", 300);
        let runner = crate::runner::AiRunner::new(std::sync::Arc::new(p), None, None);
        let input = PickDetectInput {
            takes: vec![PickCandidate {
                take_id: "t4".into(),
                take_no: 4,
                line_text: "Lunch.".into(),
                feedback_text: "I think that that one was the right one.".into(),
            }],
        };
        let out = runner.run(&input).unwrap();
        assert_eq!(out.output.unwrap().picks[0].strength, 2);
    }
}
