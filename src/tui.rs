//! Terminal progress display.
//!
//! Shows live worker status during `ql.parallel()` execution.
//! This is a simple status table, not a full TUI framework.

use crossterm::{cursor, terminal, ExecutableCommand};
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Status of a single worker.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WorkerStatus {
    Running,
    WaitingInteractive { session_name: String },
    Succeeded,
    Failed { error: String },
}

/// Message sent from a worker to the TUI render thread.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StatusUpdate {
    pub ticket_id: String,
    pub status: WorkerStatus,
    pub current_step: String,
}

/// Per-ticket result in the final summary.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TicketSummary {
    pub ticket_id: String,
    pub succeeded: bool,
    pub error: Option<String>,
    pub elapsed: Duration,
}

/// Returned after all workers finish.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RunSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub tickets: Vec<TicketSummary>,
}

// ---------------------------------------------------------------------------
// Internal row state
// ---------------------------------------------------------------------------

struct WorkerRow {
    ticket_id: String,
    status: WorkerStatus,
    current_step: String,
    started_at: Instant,
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn format_elapsed(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m{:02}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    }
}

fn format_status(status: &WorkerStatus) -> String {
    match status {
        WorkerStatus::Running => "\x1b[36mrunning\x1b[0m".to_string(),
        WorkerStatus::WaitingInteractive { .. } => {
            "\x1b[33mwaiting *\x1b[0m".to_string()
        }
        WorkerStatus::Succeeded => "\x1b[32msucceeded\x1b[0m".to_string(),
        WorkerStatus::Failed { .. } => "\x1b[31mfailed\x1b[0m".to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render(writer: &mut dyn Write, rows: &BTreeMap<String, WorkerRow>) {
    let _ = writeln!(
        writer,
        "{:<16} {:<16} {:<24} {:>8}",
        "TICKET", "STATUS", "STEP", "ELAPSED"
    );

    for row in rows.values() {
        let elapsed = format_elapsed(row.started_at.elapsed());
        let _ = writeln!(
            writer,
            "{:<16} {:<25} {:<24} {:>8}",
            truncate(&row.ticket_id, 16),
            format_status(&row.status),
            truncate(&row.current_step, 24),
            elapsed,
        );

        if let WorkerStatus::WaitingInteractive { session_name } = &row.status {
            let _ = writeln!(
                writer,
                "  \x1b[33mattach: tmux attach -t {}\x1b[0m",
                session_name
            );
        }
    }
}

fn apply_update(rows: &mut BTreeMap<String, WorkerRow>, update: StatusUpdate) {
    let entry = rows
        .entry(update.ticket_id.clone())
        .or_insert_with(|| WorkerRow {
            ticket_id: update.ticket_id.clone(),
            status: WorkerStatus::Running,
            current_step: String::new(),
            started_at: Instant::now(),
        });
    entry.status = update.status;
    entry.current_step = update.current_step;
}

/// Count lines that render() would output for a given set of rows.
fn rendered_line_count(rows: &BTreeMap<String, WorkerRow>) -> u16 {
    let mut count: u16 = 1; // header
    for row in rows.values() {
        count += 1;
        if matches!(&row.status, WorkerStatus::WaitingInteractive { .. }) {
            count += 1;
        }
    }
    count
}

// ---------------------------------------------------------------------------
// StatusDisplay — public API
// ---------------------------------------------------------------------------

/// Live terminal display of worker progress.
///
/// Spawn with `start()`, hand out senders to workers, then call `finish()`
/// to join the render thread and get a summary.
#[allow(dead_code)]
pub struct StatusDisplay {
    tx: Option<mpsc::Sender<StatusUpdate>>,
    handle: Option<thread::JoinHandle<RunSummary>>,
}

#[allow(dead_code)]
impl StatusDisplay {
    /// Spawn the background render loop. `refresh` controls how often the
    /// table redraws (e.g. `Duration::from_millis(200)`).
    pub fn start(refresh: Duration) -> Self {
        let (tx, rx) = mpsc::channel::<StatusUpdate>();
        let is_tty = io::stdout().is_terminal();

        let handle = thread::spawn(move || {
            let mut rows = BTreeMap::<String, WorkerRow>::new();
            let mut prev_lines: u16 = 0;
            let mut stdout = io::stdout();

            loop {
                // Drain all pending updates (non-blocking after the first timeout).
                match rx.recv_timeout(refresh) {
                    Ok(update) => apply_update(&mut rows, update),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }

                // Drain anything that arrived while we were busy.
                while let Ok(update) = rx.try_recv() {
                    apply_update(&mut rows, update);
                }

                // Redraw.
                if is_tty && prev_lines > 0 {
                    let _ = stdout.execute(cursor::MoveUp(prev_lines));
                    let _ = stdout.execute(terminal::Clear(
                        terminal::ClearType::FromCursorDown,
                    ));
                }

                render(&mut stdout, &rows);
                let _ = stdout.flush();
                prev_lines = rendered_line_count(&rows);
            }

            // Drain any remaining messages after disconnect.
            while let Ok(update) = rx.try_recv() {
                apply_update(&mut rows, update);
            }

            build_summary(&rows)
        });

        StatusDisplay {
            tx: Some(tx),
            handle: Some(handle),
        }
    }

    /// Clone a sender that workers use to push status updates.
    pub fn sender(&self) -> mpsc::Sender<StatusUpdate> {
        self.tx.as_ref().expect("sender called after finish").clone()
    }

    /// Drop the sender side, wait for the render thread to exit, and return
    /// the run summary. Prints a final summary table to stdout.
    pub fn finish(mut self) -> RunSummary {
        // Drop sender so the render thread sees Disconnected.
        self.tx.take();

        let summary = self
            .handle
            .take()
            .expect("finish called twice")
            .join()
            .expect("render thread panicked");

        print_summary(&summary);
        summary
    }
}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

fn build_summary(rows: &BTreeMap<String, WorkerRow>) -> RunSummary {
    let mut tickets = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for row in rows.values() {
        let ok = matches!(row.status, WorkerStatus::Succeeded);
        let err = match &row.status {
            WorkerStatus::Failed { error } => Some(error.clone()),
            _ => None,
        };
        if ok {
            succeeded += 1;
        }
        if err.is_some() {
            failed += 1;
        }
        tickets.push(TicketSummary {
            ticket_id: row.ticket_id.clone(),
            succeeded: ok,
            error: err,
            elapsed: row.started_at.elapsed(),
        });
    }

    RunSummary {
        total: tickets.len(),
        succeeded,
        failed,
        tickets,
    }
}

fn print_summary(summary: &RunSummary) {
    println!();
    println!(
        "\x1b[1m{} total | \x1b[32m{} succeeded\x1b[0m\x1b[1m | \x1b[31m{} failed\x1b[0m",
        summary.total, summary.succeeded, summary.failed
    );
    for t in &summary.tickets {
        let icon = if t.succeeded { "\x1b[32m✓\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };
        let detail = match &t.error {
            Some(e) => format!(" — {}", e),
            None => String::new(),
        };
        println!("  {} {} ({}){}", icon, t.ticket_id, format_elapsed(t.elapsed), detail);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_elapsed_seconds() {
        assert_eq!(format_elapsed(Duration::from_secs(5)), "5s");
        assert_eq!(format_elapsed(Duration::from_secs(59)), "59s");
    }

    #[test]
    fn test_format_elapsed_minutes() {
        assert_eq!(format_elapsed(Duration::from_secs(60)), "1m00s");
        assert_eq!(format_elapsed(Duration::from_secs(125)), "2m05s");
    }

    #[test]
    fn test_format_elapsed_hours() {
        assert_eq!(format_elapsed(Duration::from_secs(3661)), "1h01m01s");
    }

    #[test]
    fn test_truncate_within_limit() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exceeds_limit() {
        let result = truncate("hello world", 6);
        assert_eq!(result, "hello…");
    }

    #[test]
    fn test_apply_update_new_worker() {
        let mut rows = BTreeMap::new();
        apply_update(
            &mut rows,
            StatusUpdate {
                ticket_id: "PROJ-1".into(),
                status: WorkerStatus::Running,
                current_step: "clone".into(),
            },
        );
        assert_eq!(rows.len(), 1);
        assert!(matches!(
            rows["PROJ-1"].status,
            WorkerStatus::Running
        ));
        assert_eq!(rows["PROJ-1"].current_step, "clone");
    }

    #[test]
    fn test_apply_update_existing_worker() {
        let mut rows = BTreeMap::new();
        apply_update(
            &mut rows,
            StatusUpdate {
                ticket_id: "PROJ-1".into(),
                status: WorkerStatus::Running,
                current_step: "clone".into(),
            },
        );
        apply_update(
            &mut rows,
            StatusUpdate {
                ticket_id: "PROJ-1".into(),
                status: WorkerStatus::Succeeded,
                current_step: "done".into(),
            },
        );
        assert_eq!(rows.len(), 1);
        assert!(matches!(
            rows["PROJ-1"].status,
            WorkerStatus::Succeeded
        ));
        assert_eq!(rows["PROJ-1"].current_step, "done");
    }

    #[test]
    fn test_format_status_variants() {
        assert!(format_status(&WorkerStatus::Running).contains("running"));
        assert!(format_status(&WorkerStatus::Succeeded).contains("succeeded"));
        assert!(format_status(&WorkerStatus::Failed {
            error: "oops".into()
        })
        .contains("failed"));

        let waiting = format_status(&WorkerStatus::WaitingInteractive {
            session_name: "s1".into(),
        });
        assert!(waiting.contains("waiting *"));
    }

    #[test]
    fn test_render_includes_tmux_hint() {
        let mut rows = BTreeMap::new();
        apply_update(
            &mut rows,
            StatusUpdate {
                ticket_id: "PROJ-42".into(),
                status: WorkerStatus::WaitingInteractive {
                    session_name: "ql-PROJ-42-aider".into(),
                },
                current_step: "aider".into(),
            },
        );

        let mut buf = Vec::new();
        render(&mut buf, &rows);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("tmux attach -t ql-PROJ-42-aider"));
    }

    #[test]
    fn test_render_output_has_header() {
        let rows = BTreeMap::new();
        let mut buf = Vec::new();
        render(&mut buf, &rows);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("TICKET"));
        assert!(output.contains("STATUS"));
        assert!(output.contains("STEP"));
        assert!(output.contains("ELAPSED"));
    }

    #[test]
    fn test_status_display_lifecycle() {
        let display = StatusDisplay::start(Duration::from_millis(50));
        let tx = display.sender();

        tx.send(StatusUpdate {
            ticket_id: "T-1".into(),
            status: WorkerStatus::Running,
            current_step: "working".into(),
        })
        .unwrap();

        tx.send(StatusUpdate {
            ticket_id: "T-1".into(),
            status: WorkerStatus::Succeeded,
            current_step: "done".into(),
        })
        .unwrap();

        tx.send(StatusUpdate {
            ticket_id: "T-2".into(),
            status: WorkerStatus::Failed {
                error: "bad".into(),
            },
            current_step: "step2".into(),
        })
        .unwrap();

        drop(tx);
        let summary = display.finish();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 1);
    }
}
