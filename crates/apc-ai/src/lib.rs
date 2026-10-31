//! Cutaway AI: the provider interface, the Claude Code provider, the mock provider,
//! versioned prompts and a runner that validates, caches and logs every call.
//!
//! AI only interprets text (matching, notes, picks, spotting); it never transcribes.

pub mod claude_code;
pub mod mock;
pub mod prompts;
pub mod provider;
pub mod runner;
pub mod task;

pub use claude_code::ClaudeCodeProvider;
pub use mock::MockProvider;
pub use provider::{AiError, AiProvider, AiRequest};
pub use runner::{AiOutcome, AiRunner};
pub use task::*;
