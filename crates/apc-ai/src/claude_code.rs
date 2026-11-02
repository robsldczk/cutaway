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

fn starts(binary: &std::path::Path) -> bool {
    Command::new(binary)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `1.2.3` out of `anthropic.claude-code-1.2.3-darwin-arm64`, for picking the newest.
fn extension_version(dir: &std::path::Path) -> Vec<u64> {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.trim_start_matches("anthropic.claude-code-")
        .split('-')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Where the Claude Code CLI lives. An app started from Finder doesn't get the shell's
/// PATH, so the usual install folders are searched too, and as a last resort the CLI that
/// ships inside the VS Code / Cursor extension (same binary, same login).
pub fn find_binary() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Ok(b) = std::env::var("CUTAWAY_CLAUDE_BIN") {
        return Some(PathBuf::from(b));
    }
    let exe = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    if starts(std::path::Path::new(exe)) {
        return Some(PathBuf::from(exe));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)?;
    let mut cands: Vec<PathBuf> = vec![
        home.join(".claude/local").join(exe),
        home.join(".local/bin").join(exe),
        home.join(".npm-global/bin").join(exe),
        home.join(".bun/bin").join(exe),
        home.join(".volta/bin").join(exe),
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
    ];
    if let Some(appdata) = std::env::var_os("APPDATA") {
        cands.push(PathBuf::from(appdata).join("npm/claude.cmd"));
    }
    for editor in [".vscode", ".cursor", ".vscode-insiders"] {
        let Ok(rd) = std::fs::read_dir(home.join(editor).join("extensions")) else {
            continue;
        };
        let mut exts: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("anthropic.claude-code-"))
            })
            .collect();
        exts.sort_by_key(|p| std::cmp::Reverse(extension_version(p)));
        cands.extend(
            exts.iter()
                .map(|p| p.join("resources/native-binary").join(exe)),
        );
    }
    cands.into_iter().find(|p| p.is_file() && starts(p))
}

impl ClaudeCodeProvider {
    pub fn new(model: &str, effort: &str, timeout_s: u64) -> Self {
        ClaudeCodeProvider {
            binary: find_binary()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "claude".into()),
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
        starts(std::path::Path::new(&self.binary))
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
        // Drain stdout/stderr while the CLI runs: a reply bigger than the pipe buffer
        // (64 KiB) would otherwise block the CLI and time out, or arrive cut off.
        let drain = |s: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut buf = String::new();
                if let Some(mut s) = s {
                    let _ = s.read_to_string(&mut buf);
                }
                buf
            })
        };
        let stdout = drain(
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        );
        let stderr = drain(
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        );
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
        let status = child.wait()?;
        let out = stdout.join().unwrap_or_default();
        if !status.success() {
            let err = stderr.join().unwrap_or_default();
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

    #[test]
    fn newest_extension_sorts_first() {
        let v = |n: &str| extension_version(std::path::Path::new(n));
        assert_eq!(v("anthropic.claude-code-2.1.314-darwin-arm64"), [2, 1, 314]);
        assert!(
            v("anthropic.claude-code-2.1.314-darwin-arm64") > v("anthropic.claude-code-2.1.99")
        );
    }

    /// A reply bigger than the pipe buffer used to hang the CLI until the timeout.
    #[cfg(unix)]
    #[test]
    fn large_replies_are_read_whole() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("claude");
        let body = "x".repeat(200_000);
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\ncat > /dev/null\nprintf '%s' '{{\"result\": \"{{\\\"a\\\": \\\"{body}\\\"}}\"}}'\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut p = ClaudeCodeProvider::new("m", "low", 20);
        p.binary = bin.to_string_lossy().into_owned();
        let req = AiRequest {
            task: crate::task::AiTask::RoomClassify,
            prompt: "p".into(),
            prompt_version: 1,
            input: Value::Null,
            text: None,
            output_schema: Value::Null,
        };
        let v = p.run(&req).unwrap();
        assert_eq!(v["a"].as_str().map(str::len), Some(200_000));
    }

    #[test]
    fn notes_without_feedback_parse() {
        let o: crate::task::NoteCondenseOutput =
            serde_json::from_str(r#"{"notes": [{"take_id": "t1", "direction": "go for it"}]}"#)
                .unwrap();
        assert_eq!(o.notes[0].feedback, "");
        assert!(o.notes[0].tags.is_empty());
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
