//! Git worktree create/remove/list.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::beads::Ticket;
use crate::error::Error;

/// Default worktree directory name relative to repo root.
pub const DEFAULT_WORKTREE_DIR: &str = ".qld-worktrees";

/// Default branch prefix.
pub const DEFAULT_BRANCH_PREFIX: &str = "qld";

/// Outcome of attempting to create a worktree for a single ticket.
#[derive(Debug)]
pub enum CreateOutcome {
    /// Worktree was created successfully.
    Created {
        ticket_id: String,
        path: PathBuf,
        branch: String,
    },
    /// Worktree already existed; ticket was skipped.
    Skipped { ticket_id: String },
    /// `git worktree add` failed; ticket was skipped.
    Failed { ticket_id: String, error: Error },
}

/// Create worktrees for the given tickets.
///
/// For each ticket, derives branch `<prefix>/<ticket-id>` and creates a
/// worktree at `<worktree_dir>/<ticket-id>`. Existing worktrees are
/// skipped. Failed tickets are logged and skipped.
pub fn create(
    tickets: &[Ticket],
    repo_root: &Path,
    worktree_dir: &Path,
    branch_prefix: &str,
) -> Vec<CreateOutcome> {
    if let Err(e) = std::fs::create_dir_all(worktree_dir) {
        tracing::warn!(path = %worktree_dir.display(), error = %e, "could not create worktree directory");
    }

    tickets
        .iter()
        .map(|ticket| create_one(ticket, repo_root, worktree_dir, branch_prefix))
        .collect()
}

fn create_one(
    ticket: &Ticket,
    repo_root: &Path,
    worktree_dir: &Path,
    branch_prefix: &str,
) -> CreateOutcome {
    let branch = format!("{}/{}", branch_prefix, ticket.id);
    let path = worktree_dir.join(&ticket.id);

    // Idempotent: skip if worktree path already exists.
    if path.exists() {
        tracing::debug!(ticket_id = %ticket.id, "worktree already exists, skipping");
        return CreateOutcome::Skipped {
            ticket_id: ticket.id.clone(),
        };
    }

    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["worktree", "add", "-b", &branch])
        .arg(&path)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            tracing::info!(ticket_id = %ticket.id, path = %path.display(), "created worktree");
            CreateOutcome::Created {
                ticket_id: ticket.id.clone(),
                path,
                branch,
            }
        }
        Ok(out) => {
            let reason = String::from_utf8_lossy(&out.stderr).trim().to_string();
            tracing::warn!(ticket_id = %ticket.id, %reason, "git worktree add failed");
            CreateOutcome::Failed {
                ticket_id: ticket.id.clone(),
                error: Error::WorktreeAdd {
                    ticket_id: ticket.id.clone(),
                    reason,
                },
            }
        }
        Err(e) => {
            tracing::warn!(ticket_id = %ticket.id, error = %e, "git worktree add failed");
            CreateOutcome::Failed {
                ticket_id: ticket.id.clone(),
                error: e.into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beads::{Priority, TicketType};
    use tempfile::TempDir;

    /// Create a temporary git repo with one commit.
    fn init_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
        };
        git(&["init"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["commit", "--allow-empty", "-m", "init"]);
        dir
    }

    fn ticket(id: &str) -> Ticket {
        Ticket {
            id: id.to_string(),
            title: format!("Test ticket {}", id),
            description: None,
            priority: Priority::P1,
            ticket_type: TicketType::Task,
        }
    }

    #[test]
    fn create_single_worktree() {
        let repo = init_repo();
        let wt_dir = repo.path().join(".qld-worktrees");
        let tickets = [ticket("QLD-abc")];

        let outcomes = create(&tickets, repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);

        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            CreateOutcome::Created {
                ticket_id,
                path,
                branch,
            } => {
                assert_eq!(ticket_id, "QLD-abc");
                assert_eq!(branch, "qld/QLD-abc");
                assert!(path.exists());
                assert!(path.join(".git").exists());
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn skips_existing_worktree() {
        let repo = init_repo();
        let wt_dir = repo.path().join(".qld-worktrees");
        let tickets = [ticket("QLD-abc")];

        // Create once.
        let outcomes = create(&tickets, repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);
        assert!(matches!(&outcomes[0], CreateOutcome::Created { .. }));

        // Create again — should skip.
        let outcomes = create(&tickets, repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);
        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            CreateOutcome::Skipped { ticket_id } => {
                assert_eq!(ticket_id, "QLD-abc");
            }
            other => panic!("expected Skipped, got {other:?}"),
        }
    }

    #[test]
    fn create_multiple_worktrees() {
        let repo = init_repo();
        let wt_dir = repo.path().join(".qld-worktrees");
        let tickets = [ticket("QLD-a"), ticket("QLD-b"), ticket("QLD-c")];

        let outcomes = create(&tickets, repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);

        assert_eq!(outcomes.len(), 3);
        for outcome in &outcomes {
            assert!(matches!(outcome, CreateOutcome::Created { .. }));
        }
        assert!(wt_dir.join("QLD-a").exists());
        assert!(wt_dir.join("QLD-b").exists());
        assert!(wt_dir.join("QLD-c").exists());
    }

    #[test]
    fn custom_branch_prefix() {
        let repo = init_repo();
        let wt_dir = repo.path().join(".qld-worktrees");
        let tickets = [ticket("QLD-xyz")];

        let outcomes = create(&tickets, repo.path(), &wt_dir, "custom");

        match &outcomes[0] {
            CreateOutcome::Created { branch, .. } => {
                assert_eq!(branch, "custom/QLD-xyz");
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn creates_worktree_dir_if_missing() {
        let repo = init_repo();
        let wt_dir = repo.path().join("nested").join("worktrees");
        let tickets = [ticket("QLD-abc")];

        assert!(!wt_dir.exists());
        let outcomes = create(&tickets, repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);

        assert!(matches!(&outcomes[0], CreateOutcome::Created { .. }));
        assert!(wt_dir.join("QLD-abc").exists());
    }

    #[test]
    fn mixed_create_and_skip() {
        let repo = init_repo();
        let wt_dir = repo.path().join(".qld-worktrees");

        // Create first ticket.
        create(&[ticket("QLD-a")], repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);

        // Now create two more, including the existing one.
        let tickets = [ticket("QLD-a"), ticket("QLD-b")];
        let outcomes = create(&tickets, repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);

        assert_eq!(outcomes.len(), 2);
        assert!(matches!(&outcomes[0], CreateOutcome::Skipped { .. }));
        assert!(matches!(&outcomes[1], CreateOutcome::Created { .. }));
    }

    #[test]
    fn empty_tickets_returns_empty() {
        let repo = init_repo();
        let wt_dir = repo.path().join(".qld-worktrees");
        let outcomes = create(&[], repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);
        assert!(outcomes.is_empty());
    }

    #[test]
    fn duplicate_branch_reports_failure() {
        let repo = init_repo();
        let wt_dir = repo.path().join(".qld-worktrees");
        let tickets = [ticket("QLD-dup")];

        // Create worktree normally.
        create(&tickets, repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);

        // Remove the directory but leave the branch — simulates partial cleanup.
        let wt_path = wt_dir.join("QLD-dup");
        Command::new("git")
            .current_dir(repo.path())
            .args(["worktree", "remove", "--force"])
            .arg(&wt_path)
            .output()
            .unwrap();
        assert!(!wt_path.exists());

        // Try to create again — branch still exists, so git worktree add -b fails.
        let outcomes = create(&tickets, repo.path(), &wt_dir, DEFAULT_BRANCH_PREFIX);
        assert!(matches!(&outcomes[0], CreateOutcome::Failed { .. }));
    }
}
