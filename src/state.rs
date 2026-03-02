//! State persistence and resume logic.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TicketStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Merged,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketState {
    pub status: TicketStatus,
    pub ticket_id: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiToolConfig {
    pub name: String,
    pub command: String,
    pub timeout_secs: u64,
    pub interactive: bool,
    pub args: Vec<String>,
}

impl Default for AiToolConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            command: String::new(),
            timeout_secs: 300,
            interactive: false,
            args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunState {
    pub run_id: String,
    pub created_at: DateTime<Utc>,
    pub tickets: HashMap<String, TicketState>,
    pub ai_tools: HashMap<String, AiToolConfig>,
}
