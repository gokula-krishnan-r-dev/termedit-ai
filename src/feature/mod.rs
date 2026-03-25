pub mod ai_completion;
#[cfg(feature = "ai-server")]
pub mod ai_server;
pub mod brackets;
pub mod gemini_chat;
pub mod git_worktree;
pub mod completion;
pub mod language;
#[cfg(feature = "logs")]
pub mod logs;
pub mod outline;
pub mod search;
pub mod session;
pub mod syntax;

#[cfg(feature = "timeline")]
pub mod timeline;

#[cfg(feature = "ssh")]
pub mod ssh;