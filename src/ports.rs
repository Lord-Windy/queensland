//! Port trait definitions: `VcsProvider`, `ProcessRunner`, `ScriptRuntime`.

use std::collections::HashMap;
use std::path::PathBuf;
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
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl ProcessResult {
    pub fn success(&self) -> bool {
        self.success
    }
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub success: bool,
    pub conflicts: Vec<String>,
}

pub trait ProcessRunner: Send + Sync {
    fn run(
        &self,
        cmd: &str,
        args: &[&str],
        opts: ProcessOpts,
    ) -> Result<ProcessResult, Box<dyn std::error::Error>>;
}

pub trait VcsProvider: Send + Sync {
    fn current_branch(&self) -> Result<String, Box<dyn std::error::Error>>;
    fn commit(&self, message: &str) -> Result<(), Box<dyn std::error::Error>>;
    fn push(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn merge(&self, branch: &str) -> Result<MergeResult, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone)]
pub struct Ticket {
    pub id: String,
    pub description: String,
    pub priority: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TicketResult {
    pub ticket: Ticket,
    pub success: bool,
    pub error: Option<String>,
}

pub trait ScriptRuntime: Send + Sync {
    fn init(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn execute(
        &mut self,
        callback: &str,
        ticket: &Ticket,
    ) -> Result<TicketResult, Box<dyn std::error::Error>>;
}
