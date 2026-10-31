use serde_json::Value;
use thiserror::Error;

use crate::task::AiTask;

#[derive(Debug, Error)]
pub enum AiError {
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("provider failed: {0}")]
    Failed(String),
    #[error("timed out after {0} s")]
    Timeout(u64),
    #[error("invalid output: {0}")]
    InvalidOutput(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// One AI call: a task, its prompt (versioned), the input and the expected output schema.
#[derive(Debug, Clone)]
pub struct AiRequest {
    pub task: AiTask,
    pub prompt: String,
    pub prompt_version: u32,
    pub input: Value,
    pub output_schema: Value,
}

impl AiRequest {
    /// The full text sent to a text-in/text-out model.
    pub fn render(&self) -> String {
        format!(
            "{}\n\n## Output\nReturn only one JSON object that matches this JSON Schema, with no prose and no code fences:\n{}\n\n## Input\n{}\n",
            self.prompt.trim(),
            serde_json::to_string_pretty(&self.output_schema).unwrap_or_default(),
            serde_json::to_string_pretty(&self.input).unwrap_or_default()
        )
    }
}

/// A model that interprets text. Implementations: Claude Code CLI, mock; later the
/// Anthropic API, OpenAI-compatible endpoints or a local LLM.
pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;
    fn model(&self) -> &str;
    fn run(&self, req: &AiRequest) -> Result<Value, AiError>;
}
