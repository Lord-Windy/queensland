# Queensland — Technical Requirements Document

## 1. Overview

Queensland is an AI-powered orchestration system that automates the
software development lifecycle by coordinating issue trackers, AI coding
agents, git worktrees, and code forges into a continuous loop. It reads
unblocked tickets, farms them out to AI agents in isolated worktrees,
opens merge requests, incorporates human review feedback, and merges
completed work — then repeats.

The system is built around hexagonal (ports-and-adapters) architecture so
that every external dependency — the issue tracker, the AI editor, the
git forge, and the control interface — is a swappable port behind a trait
boundary.

## 2. Definitions

| Term | Meaning |
| --- | --- |
| **Ticket** | A unit of work from the issue tracker (initially beads) |
| **Worktree** | A `git worktree` checked out to an isolated directory for a single ticket |
| **Agent** | An AI coding tool (Claude Code, OpenCode, Aider, etc.) invoked inside a worktree |
| **Forge** | The remote hosting platform that manages MRs/PRs (GitLab, GitHub, etc.) |
| **Orchestrator** | The core loop that drives the entire workflow |
| **Run** | One complete pass of the orchestrator loop (steps 1-7) |

## 3. Goals

1. **Automate the ticket-to-merge pipeline.** A human should only need to
   write tickets and review MRs.
2. **Run many tickets in parallel.** Each ticket gets its own worktree
   and agent — they do not interfere with each other.
3. **Keep humans in the loop.** Merges only happen after human approval.
   Review comments are fed back to agents.
4. **Be adapter-agnostic from day one.** The first implementation uses
   beads + Claude Code + GitLab, but every external integration lives
   behind a trait so it can be replaced without touching the core.

## 4. Non-Goals (for v0.1)

- Web UI or dashboard (CLI only)
- Automatic conflict resolution (flag and stop on conflict)
- Multi-repo orchestration
- Cost tracking or token budgets for AI agents
- CI/CD pipeline integration (we trust the forge's own CI)

## 5. Architecture

### 5.1 Hexagonal Layout

```
                    ┌─────────────────────────────────┐
                    │          DRIVING PORTS           │
                    │  (how the outside controls us)   │
                    │                                  │
                    │   ┌─────┐   ┌──────┐   ┌─────┐  │
                    │   │ CLI │   │ HTTP │   │ TUI │  │
                    │   │     │   │(fut.)│   │(f.) │  │
                    │   └──┬──┘   └──┬───┘   └──┬──┘  │
                    │      │        │           │     │
                    └──────┼────────┼───────────┼─────┘
                           │        │           │
                    ┌──────▼────────▼───────────▼─────┐
                    │                                  │
                    │        APPLICATION CORE          │
                    │                                  │
                    │  ┌────────────────────────────┐  │
                    │  │       Orchestrator          │  │
                    │  │                            │  │
                    │  │  ┌──────────────────────┐  │  │
                    │  │  │   Ticket Processor   │  │  │
                    │  │  │   (per-ticket FSM)   │  │  │
                    │  │  └──────────────────────┘  │  │
                    │  │                            │  │
                    │  │  ┌──────────────────────┐  │  │
                    │  │  │   Merge Coordinator  │  │  │
                    │  │  └──────────────────────┘  │  │
                    │  └────────────────────────────┘  │
                    │                                  │
                    │  Domain types: Ticket, Worktree, │
                    │  MergeRequest, ReviewComment,    │
                    │  AgentResult, OrchestratorState  │
                    │                                  │
                    └──────┬────────┬───────────┬─────┘
                           │        │           │
                    ┌──────▼────────▼───────────▼─────┐
                    │          DRIVEN PORTS            │
                    │  (what we call out to)           │
                    │                                  │
                    │  ┌────────┐ ┌────────┐ ┌──────┐ │
                    │  │ Issue  │ │  Code  │ │ Git  │ │
                    │  │Tracker │ │ Agent  │ │Forge │ │
                    │  └────────┘ └────────┘ └──────┘ │
                    │  ┌────────┐ ┌────────┐          │
                    │  │Worktree│ │Notifier│          │
                    │  │Manager │ │(future)│          │
                    │  └────────┘ └────────┘          │
                    └─────────────────────────────────┘
```

### 5.2 Port Definitions (Traits)

Every external integration is defined as a Rust trait in the `domain::ports`
module. The core **never** imports concrete adapter types.

#### 5.2.1 Driving Ports

Driving ports define how external actors invoke the core. The core exposes
a service API; each driving adapter translates its own input format into
calls on that API.

```
OrchestratorService
├── run_once()          — execute one full pass of the loop
├── run_continuous()    — loop until interrupted
├── process_ticket(id)  — manually kick a single ticket
├── sync_reviews()      — pull review comments and feed back to agents
├── status()            — return current state of all in-flight tickets
└── merge_ready()       — attempt to merge all approved tickets
```

The CLI adapter (clap) is the first implementation. Future adapters (HTTP
API, TUI, Unix socket, etc.) call the same service trait.

#### 5.2.2 Driven Ports

```rust
/// Issue tracker abstraction (first impl: beads)
trait IssueTracker {
    fn list_unblocked(&self) -> Result<Vec<Ticket>>;
    fn get_ticket(&self, id: &TicketId) -> Result<Ticket>;
    fn update_status(&self, id: &TicketId, status: TicketStatus) -> Result<()>;
    fn close_ticket(&self, id: &TicketId) -> Result<()>;
}

/// AI coding agent abstraction (first impl: Claude Code CLI)
trait CodeAgent {
    fn execute(&self, task: &AgentTask) -> Result<AgentResult>;
    // AgentTask contains: ticket context, worktree path, instructions
    // AgentResult contains: success/failure, files changed, summary
}

/// Git forge abstraction (first impl: GitLab via `glab`)
trait GitForge {
    fn create_merge_request(&self, mr: &NewMergeRequest) -> Result<MergeRequestId>;
    fn get_merge_request(&self, id: &MergeRequestId) -> Result<MergeRequest>;
    fn list_comments(&self, id: &MergeRequestId) -> Result<Vec<ReviewComment>>;
    fn merge(&self, id: &MergeRequestId) -> Result<()>;
    fn close_merge_request(&self, id: &MergeRequestId) -> Result<()>;
}

/// Git worktree management (impl: shelling out to git)
trait WorktreeManager {
    fn create(&self, branch: &BranchName, path: &Path) -> Result<WorktreeHandle>;
    fn remove(&self, handle: &WorktreeHandle) -> Result<()>;
    fn list(&self) -> Result<Vec<WorktreeHandle>>;
    fn commit_and_push(&self, handle: &WorktreeHandle, message: &str) -> Result<()>;
    fn rebase_on_main(&self, handle: &WorktreeHandle) -> Result<RebaseResult>;
}
```

### 5.3 Domain Types

These live in `domain::model` and contain zero dependencies on external
crates (pure Rust types).

```rust
struct TicketId(String);
struct BranchName(String);     // derived from ticket ID, e.g. "qld/PROJ-42"
struct MergeRequestId(String);

enum TicketStatus {
    Ready,          // unblocked, not yet started
    InProgress,     // agent is working on it
    InReview,       // MR open, waiting for human
    ChangesNeeded,  // human left feedback, needs reprocessing
    Approved,       // human approved, ready to merge
    Merged,         // merged into main
    Failed,         // agent or merge failed, needs human attention
}

struct Ticket {
    id: TicketId,
    title: String,
    description: String,
    status: TicketStatus,
    branch: BranchName,
    merge_request: Option<MergeRequestId>,
    review_round: u32,
}

struct AgentTask {
    ticket: Ticket,
    worktree_path: PathBuf,
    review_comments: Vec<ReviewComment>,  // empty on first pass
    instructions: String,                 // system prompt / context
}

struct AgentResult {
    success: bool,
    summary: String,
    files_changed: Vec<PathBuf>,
}

struct ReviewComment {
    author: String,
    body: String,
    file_path: Option<PathBuf>,
    line: Option<u32>,
    timestamp: DateTime<Utc>,
}

enum RebaseResult {
    Clean,
    Conflict { files: Vec<PathBuf> },
}
```

## 6. The Orchestrator Loop

The orchestrator is the beating heart of Queensland. It runs as a
finite-state machine per ticket, coordinated by a central loop.

### 6.1 Per-Ticket State Machine

```
                 ┌──────────┐
                 │  Ready   │◄─────────── ticket unblocked
                 └────┬─────┘
                      │ create worktree + branch
                      ▼
                ┌────────────┐
                │ InProgress │◄──── agent working
                └─────┬──────┘
                      │ agent done, push, open MR
                      ▼
                ┌────────────┐
                │  InReview  │◄──── waiting for human
                └─────┬──────┘
                      │
              ┌───────┴────────┐
              ▼                ▼
     ┌──────────────┐   ┌──────────┐
     │ChangesNeeded │   │ Approved │
     └──────┬───────┘   └────┬─────┘
            │                │
            │ feed comments  │ rebase on main
            │ back to agent  │ merge MR
            │                │ remove worktree
            ▼                ▼
       InProgress         ┌────────┐
       (loop back)        │ Merged │
                          └────────┘

     Any state can transition to Failed on unrecoverable error.
```

### 6.2 Orchestrator Pseudocode

```
fn run_once():
    // Phase 1: Discover
    tickets = issue_tracker.list_unblocked()
    for ticket in tickets where ticket has no worktree:
        worktree = worktree_manager.create(ticket.branch)
        ticket.status = InProgress

    // Phase 2: Execute agents (parallelizable)
    for ticket in tickets where status == InProgress:
        task = build_agent_task(ticket)
        result = code_agent.execute(task)
        if result.success:
            worktree_manager.commit_and_push(ticket.worktree)
            if ticket.merge_request is None:
                mr = forge.create_merge_request(ticket)
                ticket.merge_request = Some(mr)
            ticket.status = InReview
        else:
            ticket.status = Failed

    // Phase 3: Sync reviews
    for ticket in tickets where status == InReview:
        comments = forge.list_comments(ticket.merge_request)
        if has_reprocess_signal(comments):
            ticket.review_comments = extract_feedback(comments)
            ticket.status = ChangesNeeded
        if has_approval_signal(comments):
            ticket.status = Approved

    // Phase 4: Reprocess (feeds back into Phase 2 next run)
    for ticket in tickets where status == ChangesNeeded:
        ticket.status = InProgress  // will be picked up in Phase 2

    // Phase 5: Merge
    for ticket in tickets where status == Approved:
        rebase_result = worktree_manager.rebase_on_main(ticket.worktree)
        match rebase_result:
            Clean => {
                forge.merge(ticket.merge_request)
                worktree_manager.remove(ticket.worktree)
                issue_tracker.close_ticket(ticket.id)
                ticket.status = Merged
            }
            Conflict { .. } => {
                ticket.status = Failed  // needs human attention
            }
```

### 6.3 Concurrency Model

The orchestrator runs agents in parallel (one tokio task per ticket in
Phase 2). All other phases are sequential to avoid race conditions with
git and the forge API.

The number of concurrent agents is configurable
(`--max-concurrent-agents`, default 4). This bounds both resource usage
and API costs.

## 7. Adapter Specifications

### 7.1 Issue Tracker: Beads

The beads adapter shells out to the `bd` CLI (or reads the `.beads/`
directory directly from the filesystem).

| Port method | Implementation |
| --- | --- |
| `list_unblocked()` | `bd ready` — parse output |
| `get_ticket(id)` | `bd show <id>` — parse output |
| `update_status(id, s)` | `bd update <id> --status <s>` |
| `close_ticket(id)` | `bd close <id>` |

The beads adapter is explicitly designed to be replaced. The trait
boundary ensures that switching to Jira, Linear, GitHub Issues, or a
custom tracker requires only a new adapter — zero changes to the core.

### 7.2 Code Agent: Claude Code

The Claude Code adapter invokes `claude` in a subprocess within the
worktree directory.

```
claude --print \
    --dangerously-skip-permissions \
    --working-dir <worktree_path> \
    "<prompt built from ticket + review comments>"
```

The prompt is assembled by the core from the ticket description and any
accumulated review feedback. The adapter is only responsible for
invocation and capturing the result.

Alternative adapters: OpenCode (`opencode`), Aider (`aider`), Codex
(`codex`), or a direct API integration.

### 7.3 Git Forge: GitLab

The GitLab adapter uses `glab` (GitLab CLI).

| Port method | Implementation |
| --- | --- |
| `create_merge_request()` | `glab mr create --source-branch <b> --title <t> --description <d>` |
| `get_merge_request()` | `glab mr view <id> --json` |
| `list_comments()` | `glab mr note list <id>` or GitLab API via `glab api` |
| `merge()` | `glab mr merge <id> --squash` (configurable) |
| `close_merge_request()` | `glab mr close <id>` |

**Review signals:**
- "Ready for reprocessing" (or a configurable label/keyword) in a comment
  triggers the `ChangesNeeded` transition.
- Merge approval (GitLab approval status or a configurable keyword)
  triggers the `Approved` transition.

Alternative adapters: GitHub (`gh`), Gitea, Forgejo, BitBucket.

### 7.4 Worktree Manager: Git

Shells out to `git`.

| Port method | Implementation |
| --- | --- |
| `create(branch, path)` | `git worktree add -b <branch> <path>` |
| `remove(handle)` | `git worktree remove <path> && git branch -d <branch>` |
| `list()` | `git worktree list --porcelain` |
| `commit_and_push(h, msg)` | `git -C <path> add -A && git -C <path> commit -m <msg> && git -C <path> push -u origin <branch>` |
| `rebase_on_main(h)` | `git -C <path> fetch origin main && git -C <path> rebase origin/main` |

Worktrees are created under a configurable directory (default:
`<repo>/.qld-worktrees/`). This directory is added to `.gitignore`.

## 8. Project Structure

```
queensland/
├── Cargo.toml
├── src/
│   ├── main.rs                     # entry point — wires adapters, runs CLI
│   │
│   ├── domain/                     # THE CORE — no external dependencies
│   │   ├── mod.rs
│   │   ├── model.rs                # Ticket, AgentTask, MergeRequest, etc.
│   │   ├── ports/
│   │   │   ├── mod.rs
│   │   │   ├── issue_tracker.rs    # trait IssueTracker
│   │   │   ├── code_agent.rs       # trait CodeAgent
│   │   │   ├── git_forge.rs        # trait GitForge
│   │   │   └── worktree_manager.rs # trait WorktreeManager
│   │   ├── orchestrator.rs         # the core loop
│   │   ├── ticket_processor.rs     # per-ticket FSM logic
│   │   └── merge_coordinator.rs    # merge sequencing logic
│   │
│   ├── adapters/                   # ADAPTERS — each implements a port
│   │   ├── mod.rs
│   │   ├── beads.rs                # IssueTracker for beads
│   │   ├── claude_code.rs          # CodeAgent for Claude Code
│   │   ├── gitlab.rs               # GitForge for GitLab
│   │   ├── git_worktree.rs         # WorktreeManager via git CLI
│   │   └── cli.rs                  # Driving adapter: clap CLI
│   │
│   ├── config.rs                   # Configuration loading
│   └── error.rs                    # Error types
│
├── tests/
│   ├── integration/                # tests using fake/mock adapters
│   └── e2e/                        # full end-to-end with real git repos
│
└── docs/
    └── trd.md                      # this document
```

### 8.1 Dependency Rule

The hexagonal dependency rule is **strict**:

```
adapters → domain     ✅  adapters depend on domain traits
domain   → adapters   ❌  domain NEVER imports from adapters
main.rs  → both       ✅  main.rs is the composition root
```

The `domain/` module has **zero** dependencies on external crates. It
uses only `std` and types defined within itself. All trait methods return
`Result<T, DomainError>` where `DomainError` is defined in the domain.

Adapters depend on external crates (`clap`, `serde`, `tokio`,
`glab`/`gh` CLIs, etc.) and implement domain traits.

`main.rs` is the **composition root** — it constructs concrete adapters
and injects them into the core. This is the only place where concrete
adapter types and domain types meet.

## 9. Configuration

Queensland is configured via a TOML file (`queensland.toml` in the repo
root) with CLI overrides.

```toml
[queensland]
worktree_dir = ".qld-worktrees"    # relative to repo root
max_concurrent_agents = 4
branch_prefix = "qld"              # branches: qld/<ticket-id>

[issue_tracker]
adapter = "beads"                  # "beads" | "github" | "jira" | ...

[code_agent]
adapter = "claude-code"            # "claude-code" | "opencode" | "aider" | ...
# Agent-specific config
[code_agent.claude_code]
model = "sonnet"
dangerously_skip_permissions = true

[forge]
adapter = "gitlab"                 # "gitlab" | "github" | ...
merge_strategy = "squash"          # "squash" | "merge" | "rebase"
reprocess_signal = "ready for reprocessing"
approval_signal = "approved"       # or use forge-native approvals

[forge.gitlab]
# glab-specific overrides if needed
```

## 10. CLI Interface

```
queensland [OPTIONS] <COMMAND>

Commands:
  run        Run the orchestrator loop
  status     Show status of all in-flight tickets
  process    Manually process a single ticket
  sync       Sync review comments from the forge
  merge      Attempt to merge all approved tickets
  config     Show resolved configuration

Run options:
  --once             Run a single pass then exit (default)
  --watch            Run continuously, polling at interval
  --interval <secs>  Polling interval for watch mode (default: 300)
  --max-agents <n>   Max concurrent agents (default: 4)
  --dry-run          Show what would happen without doing it

Global options:
  --config <path>    Path to config file (default: queensland.toml)
  --verbose          Increase log verbosity
  --quiet            Suppress non-error output
```

## 11. Error Handling Strategy

Errors are categorized by recoverability:

| Category | Example | Behaviour |
| --- | --- | --- |
| **Transient** | Network timeout, git lock | Retry with backoff (max 3) |
| **Ticket-scoped** | Agent fails, merge conflict | Mark ticket as `Failed`, continue other tickets |
| **Fatal** | Can't access repo, bad config | Abort the entire run with clear error message |

A failed ticket never blocks the orchestrator. The human reviews failed
tickets and either fixes the issue manually or adjusts the ticket and
re-triggers.

## 12. Testing Strategy

### 12.1 Unit Tests (domain/)

The domain is pure logic with trait-based dependencies. Tests inject
mock/fake implementations of every port.

```rust
struct FakeIssueTracker { tickets: Vec<Ticket> }
impl IssueTracker for FakeIssueTracker { ... }

struct FakeCodeAgent { always_succeeds: bool }
impl CodeAgent for FakeCodeAgent { ... }
```

These tests validate:
- The orchestrator loop transitions tickets through the correct states
- The per-ticket FSM handles all edge cases
- Merge ordering is correct (sequential, respects dependencies)

### 12.2 Integration Tests (adapters)

Each adapter is tested against its real external dependency in a
controlled environment:
- `beads` adapter: real beads in a temp git repo
- `git_worktree` adapter: real git operations in a temp repo
- `gitlab` adapter: mock HTTP server or a test GitLab project

### 12.3 End-to-End Tests

A full run against a test repository with real git, real beads, and
a mock forge. Validates the complete loop from ticket discovery to merge.

## 13. Security Considerations

- **Agent sandboxing:** AI agents run with `--dangerously-skip-permissions`
  inside isolated worktrees. The worktree directory should be considered
  untrusted. Queensland itself never executes code produced by agents.
- **Credential handling:** Queensland never stores credentials. It relies
  on the forge CLI's own auth (`glab auth`, `gh auth`) and the agent's
  own API key handling.
- **Prompt injection:** Ticket descriptions and review comments are
  passed to agents. Queensland does not attempt to sanitize these — the
  agent is responsible for its own safety. However, Queensland should log
  all prompts sent to agents for auditability.

## 14. Future Extensions (Not in v0.1)

These are explicitly out of scope but the architecture accommodates them:

- **Web dashboard** — a new driving adapter (HTTP/WebSocket) calling the
  same `OrchestratorService` trait
- **TUI** — another driving adapter using `ratatui` or similar
- **Notification adapter** — Slack/email/webhook notifications on state
  changes
- **Cost tracking** — decorator around `CodeAgent` that meters token
  usage
- **Multi-repo** — orchestrator manages multiple repo roots
- **Conflict resolution agent** — a specialized `CodeAgent` invoked when
  rebase fails, instead of immediately marking as `Failed`
- **Pipeline gating** — wait for CI to pass before allowing merge
- **Dependency-aware merge ordering** — if ticket B depends on ticket A,
  merge A first

## 15. Open Questions

1. **Worktree lifetime:** Should worktrees persist across runs (resume
   agent work) or be ephemeral (recreated each run)? The current design
   assumes persistent worktrees that survive across runs.

2. **Agent prompt engineering:** How much context should Queensland
   inject? Options range from "just the ticket description" to "ticket +
   full codebase summary + review history + related tickets."

3. **Merge ordering:** When multiple tickets are approved simultaneously,
   what merge order? Options: ticket ID order, dependency order, or
   smallest-diff-first to minimize conflict surface.

4. **State persistence:** The orchestrator needs to track ticket state
   across runs. Options: (a) derive entirely from git + forge state on
   each run (stateless), (b) persist to a local SQLite/JSON file, (c)
   store state in beads metadata. The stateless approach is simplest and
   most robust.

## 16. Dependencies (Cargo)

Minimal initial dependency set:

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
tokio = { version = "1", features = ["full"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

No GitLab/GitHub API client crates — we shell out to `glab`/`gh` to
keep the adapters thin and avoid API version coupling.

## 17. Milestones

### M1: Skeleton + Domain Types
- Project structure with hexagonal layout
- All domain types and port traits defined
- Fake adapters for testing
- Orchestrator loop compiles and runs with fakes

### M2: Git Worktree Adapter
- Real `WorktreeManager` implementation
- Create, list, remove worktrees
- Commit, push, rebase operations
- Integration tests with real git

### M3: Beads Adapter
- Real `IssueTracker` implementation
- Parse beads output
- List unblocked, update status, close

### M4: Claude Code Adapter
- Real `CodeAgent` implementation
- Prompt construction from ticket + review context
- Subprocess management and output capture

### M5: GitLab Adapter
- Real `GitForge` implementation via `glab`
- Create MR, list comments, detect signals, merge

### M6: CLI + Config
- `clap` CLI with all commands
- TOML config loading
- Composition root wiring in `main.rs`

### M7: End-to-End Integration
- Full loop: discover → agent → MR → review → merge
- Error handling and retry logic
- Logging and observability

### M8: Polish
- `--watch` mode with polling
- `--dry-run` mode
- Documentation
- Release packaging
