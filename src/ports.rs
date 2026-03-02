//! Port trait definitions: `VcsProvider`, `ProcessRunner`, `ScriptRuntime`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProcessOpts {
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub timeout: Option<Duration>,
}

impl Default for ProcessOpts {
    fn default() -> Self {
        Self {
            cwd: None,
            env: HashMap::new(),
            timeout: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub success: bool,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Ticket {
    pub key: String,
    pub summary: String,
    pub fields: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct TicketResult {
    pub ticket: Ticket,
    pub success: bool,
    pub error: Option<String>,
}

/// Process spawning and supervision — v1 adapter: OsProcess
pub trait ProcessRunner: Send + Sync {
    fn run(&self, cmd: &str, args: &[&str], opts: ProcessOpts)
        -> Result<ProcessResult, Box<dyn std::error::Error>>;
}

/// VCS operations — v1 adapter: GitCli
pub trait VcsProvider: Send + Sync {
    fn worktree_add(&self, branch: &str) -> Result<PathBuf, Box<dyn std::error::Error>>;
    fn worktree_remove(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>>;
    fn commit(&self, cwd: &Path, message: &str) -> Result<(), Box<dyn std::error::Error>>;
    fn push(&self, cwd: &Path, branch: &str) -> Result<(), Box<dyn std::error::Error>>;
    fn merge(&self, branch: &str) -> Result<MergeResult, Box<dyn std::error::Error>>;
    fn current_branch(&self) -> Result<String, Box<dyn std::error::Error>>;
}

/// Scripting runtime — v1 adapter: LuaRuntime
pub trait ScriptRuntime {
    fn load(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>>;
    fn call_parallel(
        &self,
        items: Vec<Ticket>,
        concurrency: usize,
        vcs: &dyn VcsProvider,
        process: &dyn ProcessRunner,
    ) -> Result<Vec<TicketResult>, Box<dyn std::error::Error>>;
}
