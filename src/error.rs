//! Error types.

use std::path::PathBuf;

/// Result alias for queensland operations.
pub type Result<T> = std::result::Result<T, Error>;

/// All error conditions that can occur during queensland operation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("queensland must be run inside a tmux session")]
    NotInTmux,

    #[error("no ready tickets found after filtering")]
    NoReadyTickets,

    #[error("`bd` CLI not found — install beads: https://github.com/samuelbrownGE/beads")]
    BdNotFound,

    #[error("git worktree add failed for {ticket_id}: {reason}")]
    WorktreeAdd {
        ticket_id: String,
        reason: String,
    },

    #[error("AI editor binary not found: {binary} — configure [agent].command in queensland.toml")]
    AgentNotFound { binary: String },

    #[error("failed to parse config file {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
