# Queensland — Technical Requirements Document

## 1. Overview

Queensland is a CLI tool that reads tickets from beads, creates isolated
git worktrees for each one, and opens tmux tabs with an AI editor pointed
at each worktree to work the ticket. The human reviews the results,
commits what they want, and cleans up.

This is the minimal viable version. Future iterations will add forge
integration, automated MRs, review feedback loops, and merge
orchestration.

## 2. Definitions

| Term | Meaning |
| --- | --- |
| **Ticket** | A unit of work from beads (an issue with an ID, title, description, priority, and type) |
| **Worktree** | A `git worktree` checked out to an isolated directory for a single ticket |
| **Agent** | An AI coding tool (Claude Code initially) launched in a tmux tab to work a ticket |

## 3. Goals

1. **Create worktrees from beads tickets.** Query beads for ready
   tickets, optionally filtered by priority or type, and create a git
   worktree + branch for each.
2. **Launch AI agents in tmux tabs.** For each worktree, open a new tmux
   tab running an AI editor that is instructed to complete the ticket.
   The agent should not commit.

## 4. Non-Goals (for this version)

- Forge integration (no MR/PR creation)
- Automated merging or rebasing
- Review feedback loops
- Continuous orchestrator loop
- Web UI, TUI, or dashboard
- Cost tracking or token budgets
- Multi-repo orchestration

## 5. Workflow

```
  ┌─────────────────────┐
  │  queensland run      │
  │  --priority P0       │  (optional filters)
  │  --type TASK         │
  └──────────┬──────────┘
             │
             ▼
  ┌─────────────────────┐
  │  Query beads for     │
  │  ready tickets       │
  │  (bd ready)          │
  └──────────┬──────────┘
             │
             ▼
  ┌─────────────────────┐
  │  Filter by priority  │
  │  and/or type         │
  └──────────┬──────────┘
             │
             ▼
  ┌─────────────────────┐
  │  For each ticket:    │
  │                      │
  │  1. Create worktree  │
  │     git worktree add │
  │     -b qld/<id>      │
  │                      │
  │  2. Open tmux tab    │
  │     named <id>       │
  │                      │
  │  3. Launch AI editor │
  │     in the worktree  │
  │     with ticket as   │
  │     the prompt       │
  └─────────────────────┘
             │
             ▼
  ┌─────────────────────┐
  │  Human reviews each  │
  │  tab, commits or     │
  │  discards as needed  │
  └─────────────────────┘
```

## 6. Feature 1: Worktree Creation

### 6.1 Ticket Discovery

Queensland queries beads for ready (unblocked) tickets using `bd ready`.
The output is parsed to extract ticket IDs, titles, descriptions,
priorities, and types.

### 6.2 Filtering

The user can filter which tickets get worktrees:

- **By priority:** `--priority P0` or `--priority P0,P1` — only create
  worktrees for tickets at the specified priority levels.
- **By type:** `--type TASK` or `--type TASK,BUG` — only create
  worktrees for tickets of the specified types.
- Filters combine with AND logic. `--priority P0 --type TASK` means
  "P0 tasks only."
- With no filters, all ready tickets get worktrees.

### 6.3 Worktree Creation

For each matching ticket:

1. Derive branch name: `qld/<ticket-id>` (e.g. `qld/QLD-7`).
2. Create the worktree:
   `git worktree add -b qld/<ticket-id> <worktree-dir>/<ticket-id>`
3. Skip tickets that already have a worktree (idempotent).

Worktrees are created under a configurable directory (default:
`<repo>/.qld-worktrees/`). This directory should be in `.gitignore`.

### 6.4 Cleanup

A separate `queensland clean` command removes worktrees:

```
queensland clean              # remove all qld worktrees
queensland clean <ticket-id>  # remove a specific worktree
```

This runs `git worktree remove <path> && git branch -d <branch>` for
each.

## 7. Feature 2: Tmux Tab Spawning with AI Editor

### 7.1 Tmux Integration

For each worktree created (or already existing), Queensland opens a new
tmux window (tab) in the current tmux session:

1. Create a new tmux window named after the ticket ID.
2. Set the working directory to the worktree path.
3. Launch the AI editor with the ticket prompt.

Queensland must be run from inside a tmux session. If not, it should
error with a clear message.

### 7.2 AI Editor Invocation

The AI editor is Claude Code initially. It is launched with:

```
claude --dangerously-skip-permissions \
    "<ticket-title>: <ticket-description>. Do not commit."
```

The prompt is constructed from the ticket's title and description, with
an explicit instruction not to commit. The human will review the changes
and decide what to commit.

The AI editor is configurable for future swapping (OpenCode, Aider,
etc.) but Claude Code is the only implementation for now.

### 7.3 Idempotency

If a tmux window for a ticket already exists, Queensland should skip it
rather than opening a duplicate. Check for existing windows by name
before creating new ones.

## 8. CLI Interface

```
queensland <COMMAND> [OPTIONS]

Commands:
  run        Create worktrees and launch AI agents in tmux tabs
  clean      Remove worktrees and their branches
  status     Show current worktrees and their ticket associations

Run options:
  --priority <levels>  Filter by priority (e.g. P0 or P0,P1)
  --type <types>       Filter by type (e.g. TASK or TASK,BUG)
  --dry-run            Show what would happen without doing it
  --worktree-only      Create worktrees but don't open tmux tabs

Clean options:
  <ticket-id>          Remove a specific worktree (omit for all)

Global options:
  --config <path>      Path to config file (default: queensland.toml)
  --verbose            Increase log verbosity
  --quiet              Suppress non-error output
```

## 9. Configuration

```toml
[queensland]
worktree_dir = ".qld-worktrees"    # relative to repo root
branch_prefix = "qld"              # branches: qld/<ticket-id>

[agent]
command = "claude"                 # the AI editor binary
args = ["--dangerously-skip-permissions"]
no_commit_instruction = "Do not commit."
```

## 10. Project Structure

```
queensland/
├── Cargo.toml
├── src/
│   ├── main.rs              # entry point, CLI parsing, wiring
│   ├── beads.rs             # query and parse beads tickets
│   ├── worktree.rs          # git worktree create/remove/list
│   ├── tmux.rs              # tmux window create/check/launch
│   ├── agent.rs             # AI editor prompt construction + launch
│   ├── config.rs            # TOML config loading
│   └── error.rs             # error types
│
├── tests/
│   └── integration/         # tests with real git repos + beads
│
└── docs/
    └── trd.md               # this document
```

This is a flat, simple structure. No hexagonal architecture for now —
that can be introduced when forge integration and the full orchestrator
loop are added.

## 11. Error Handling

| Situation | Behaviour |
| --- | --- |
| Not inside a tmux session | Error with message: "queensland must be run inside a tmux session" |
| No ready tickets (after filtering) | Print message and exit cleanly |
| `bd` CLI not found | Error with message pointing to beads installation |
| Worktree already exists for ticket | Skip silently (log at verbose level) |
| Tmux window already exists for ticket | Skip silently (log at verbose level) |
| `git worktree add` fails | Log error, skip ticket, continue with others |
| AI editor binary not found | Error with message about configuring the agent |

## 12. Dependencies (Cargo)

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
```

No async runtime needed — everything is synchronous subprocess calls
(`git`, `bd`, `tmux`, `claude`).

## 13. Milestones

### M1: Beads Integration
- Parse `bd ready` output
- Filter tickets by priority and type
- Unit tests with sample beads output

### M2: Worktree Management
- Create worktrees from ticket list
- List and remove worktrees
- Idempotent creation (skip existing)
- Integration tests with real git

### M3: Tmux + Agent Launch
- Detect tmux session
- Create named windows with correct working directory
- Launch Claude Code with ticket prompt
- Idempotent window creation (skip existing)

### M4: CLI + Config
- `clap` CLI with run/clean/status commands
- TOML config loading
- `--dry-run` mode
- End-to-end manual testing

## 14. Future Extensions

These are out of scope now but will be added once the basics work:

- **Forge integration** — create MRs/PRs from worktree branches
- **Review feedback loop** — feed review comments back to agents
- **Automated merging** — merge approved MRs, handle conflicts
- **Continuous mode** — run as a loop, polling for new tickets
- **Hexagonal architecture** — trait boundaries for swappable adapters
- **Multiple AI editors** — OpenCode, Aider, Codex adapters
