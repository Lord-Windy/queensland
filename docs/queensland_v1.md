# QUEENSLAND

**Technical Requirements Document**
*Parallel AI Task Orchestration Engine*

Version 1.0 · March 2026 · Status: Draft

---

## Table of Contents

1. [Overview](#1-overview)
2. [Goals and Non-Goals](#2-goals-and-non-goals)
3. [Architecture](#3-architecture)
4. [Lua Runtime and API Surface](#4-lua-runtime-and-api-surface)
5. [Host Binary Responsibilities](#5-host-binary-responsibilities)
6. [State Management and Resumability](#6-state-management-and-resumability)
7. [Prompt Templating](#7-prompt-templating)
8. [Error Handling](#8-error-handling)
9. [Merge Strategy](#9-merge-strategy)
10. [CLI Interface](#10-cli-interface)
11. [Project Structure](#11-project-structure)
12. [Data Structures](#12-data-structures)
13. [Implementation Phases](#13-implementation-phases)
14. [Open Questions](#14-open-questions)
15. [Appendix A: Example queensland.lua](#appendix-a-example-queenslandlua)

---

## 1. Overview

Queensland is a parallel AI task orchestration engine. It coordinates the execution of multiple AI-assisted development tasks concurrently, managing the full lifecycle from ticket ingestion through implementation, review, and merge.

The core architecture is a Rust host binary embedding a Lua scripting runtime. Users define their workflows as Lua templates that call into a host-provided standard library. This gives users full programmatic control over their pipeline without requiring them to build and maintain a compiled application.

**Core workflow:** Fetch a batch of tasks from any source, fan out to N parallel workers (each in its own git worktree), run AI tools and conventional tooling per-task, then sequentially merge results back to the main branch.

---

## 2. Goals and Non-Goals

### 2.1 Goals

- Parallel execution of AI-assisted development tasks with configurable concurrency
- Resumable and recoverable: surviving crashes, restarts, and partial failures gracefully
- User-defined workflows via Lua templates with no DSL or config-language limitations
- Pluggable ticket sources via callback functions (not hard-coded integrations)
- Pluggable AI tool interface via callback functions with per-tool flag customization
- Prompt templating with global and local scopes
- Sequential merge with interactive AI-assisted conflict resolution
- Clean state tracking with the ability to inspect, resume, or clean up any run

### 2.2 Non-Goals for v1

- Web UI or dashboard
- Multi-repository support
- Built-in integrations for specific ticket systems (Jira, Linear, etc.)
- Distributed execution across multiple machines
- Plugin system beyond Lua callbacks
- Real-time collaboration or multi-user support

---

## 3. Architecture

Queensland follows an embedded scripting architecture. The Rust host binary provides system-level capabilities (process management, git operations, concurrency control, filesystem access) and exposes them as a Lua standard library. The user writes a Lua template that defines the workflow and lives in the project directory.

### 3.1 Component Overview

| Component | Language | Responsibility |
|-----------|----------|----------------|
| Host binary (`queensland`) | Rust | Process lifecycle, concurrency, git ops, state persistence, TUI, Lua runtime |
| Lua runtime (embedded) | Lua 5.4 via mlua | Executes user templates, calls host functions |
| User template (`queensland.lua`) | Lua | Defines pipeline: ticket source, task steps, prompts, merge strategy |
| Prompt templates (`prompts/`) | Markdown | Reusable prompt files with variable interpolation |
| State file (`queensland.state.json`) | JSON | Tracks run status for resumability |

### 3.2 Execution Model

When the user runs `queensland`, the following sequence occurs:

1. The host binary starts and loads `queensland.lua` from the current directory.
2. The Lua template calls host functions to fetch tickets, configure concurrency, and define the pipeline.
3. `ql.parallel()` fans out tickets to a worker pool managed by Rust. Each worker gets its own OS thread and git worktree.
4. Within each worker, Lua callbacks execute sequentially: AI tool invocations, shell commands, file operations.
5. On completion (or failure), each worker reports status back. Failed tasks are paused with diagnostic notes left in the worktree.
6. After all parallel work completes, the merge phase runs sequentially. If conflicts arise, an interactive AI-assisted resolution session begins.
7. State is persisted to disk after every significant transition.

---

## 4. Lua Runtime and API Surface

The Lua API is the primary interface users interact with. It is exposed as the `ql` module (required via `require("queensland")`). All host capabilities are accessed through this module. The API is designed to be small, composable, and unsurprising.

### 4.1 Core API

| Function | Description |
|----------|-------------|
| `ql.parallel(items, fn)` | Execute `fn` for each item with concurrency control. The core orchestration primitive. |
| `ql.concurrency` | Property. Max parallel workers (default: 4). |
| `ql.exec(cwd, cmd, ...args)` | Run a shell command synchronously. Returns `{success, stdout, stderr, exit_code}`. |
| `ql.log(level, msg)` | Structured logging (`info`, `warn`, `error`, `debug`). |
| `ql.prompt(path, vars)` | Load a prompt template file and interpolate variables. |
| `ql.sleep(seconds)` | Pause execution. |
| `ql.env(name)` | Read environment variable. |

### 4.2 Git Operations (`ql.git`)

| Function | Description |
|----------|-------------|
| `ql.git.worktree_add(branch)` | Create a worktree for the given branch. Returns the worktree path. |
| `ql.git.worktree_remove(path)` | Remove a worktree and clean up. |
| `ql.git.commit(cwd, message)` | Stage all and commit in the given directory. |
| `ql.git.push(cwd, branch)` | Push branch to remote. |
| `ql.git.merge(branch)` | Merge branch into current branch. Returns `{success, conflicts}`. |
| `ql.git.merge_all(branches)` | Sequential merge of all branches. Invokes conflict resolver on failure. |
| `ql.git.current_branch()` | Return the current branch name. |

### 4.3 AI Tool Interface (`ql.ai`)

The AI interface is deliberately thin. It spawns a process, feeds it a prompt, waits for it to finish, and returns the result. Tool-specific behavior is handled via user-defined callback functions.

| Function | Description |
|----------|-------------|
| `ql.ai.register(name, config)` | Register an AI tool with its default flags, binary path, and argument pattern. |
| `ql.ai.run(opts)` | Run a registered AI tool. `opts: {tool, cwd, prompt, args?, timeout?, on_output?}`. Returns `{success, output, exit_code, duration}`. |

Example of registering tools with different default configurations:

```lua
ql.ai.register("claude", {
    bin = "claude",
    default_args = { "--dangerously-skip-permissions" },
    prompt_flag = "--prompt",  -- or nil if prompt goes to stdin
    timeout = 600,
})

ql.ai.register("opencode", {
    bin = "opencode",
    default_args = {},
    prompt_flag = nil,  -- prompt as positional arg
    timeout = 900,
})
```

### 4.4 Ticket Source Interface (`ql.tickets`)

Queensland does not ship with built-in ticket source integrations. Instead, the user implements a fetch callback. This keeps the tool decoupled from the rapidly-changing landscape of project management tools.

```lua
-- User implements this in their template
function fetch_tickets()
    -- Option A: Shell out to a CLI tool
    local result = ql.exec(".", "bd", "ready", "--json")
    return ql.json.decode(result.stdout)

    -- Option B: Read from a local file
    -- return ql.json.decode(ql.fs.read("tickets.json"))

    -- Option C: Use a shared library (if someone publishes one)
    -- local linear = require("queensland-linear")
    -- return linear.fetch_ready()
end
```

The expected return type is a list of ticket objects. Queensland requires only two fields: `key` (a unique identifier used for branch naming) and `summary` (a human-readable title). All other fields are passed through to prompt templates untouched.

| Field | Required | Description |
|-------|----------|-------------|
| `key` | Yes | Unique identifier, used for branch names (e.g., `PROJ-123`, `issue-42`) |
| `summary` | Yes | Human-readable title |
| `description` | No | Full description/body |
| `notes` | No | Additional context or notes |
| `labels` | No | List of labels/tags |
| `*` | No | Any additional fields are passed through to prompt templates |

---

## 5. Host Binary Responsibilities

The Rust host binary handles everything that is unsafe, complex, or performance-sensitive in a scripting language. It is the foundation that makes the Lua templates simple.

### 5.1 Concurrency Manager

The concurrency manager is a semaphore-gated thread pool. When `ql.parallel()` is called from Lua, the host spawns up to N worker threads (configurable via `ql.concurrency`, default 4). Each worker executes the user-provided Lua callback in its own Lua state, with access to the full `ql` API. The host manages worker lifecycle, captures panics and errors, and aggregates results.

### 5.2 Process Supervisor

Every external process (AI tools, shell commands) is spawned and managed by the host. The supervisor handles: spawning with the correct working directory and environment, capturing stdout/stderr in real time, enforcing timeouts with graceful shutdown (SIGTERM then SIGKILL), returning structured results to Lua, and logging all process activity for debugging.

### 5.3 Git Manager

All git operations go through a centralized manager that ensures worktrees do not collide, branches are created from the correct base, and cleanup happens even on failure. The git manager maintains an internal registry of active worktrees and their associated branches.

### 5.4 TUI / Progress Display

During parallel execution, the host renders a terminal UI showing the status of each worker: which ticket it is processing, which step it is on, elapsed time, and whether it has succeeded, failed, or is still running. This is a simple status table, not a full TUI framework. After completion, a summary is printed showing results for each ticket.

---

## 6. State Management and Resumability

State management is critical. Queensland must survive laptop restarts, network failures, and partial crashes without leaving the repository in an unrecoverable state.

### 6.1 State File

Queensland writes a `queensland.state.json` file in the project root after every significant state transition. This file is the source of truth for resumability.

```json
{
  "run_id": "20260301-143022-a7f3",
  "started_at": "2026-03-01T14:30:22Z",
  "base_branch": "main",
  "concurrency": 4,
  "template": "queensland.lua",
  "tickets": {
    "PROJ-101": {
      "status": "completed",
      "branch": "proj-101",
      "worktree": "./worktrees/proj-101",
      "started_at": "2026-03-01T14:30:23Z",
      "completed_at": "2026-03-01T14:35:47Z",
      "current_step": "review",
      "steps_completed": ["implement", "test", "review"]
    },
    "PROJ-102": {
      "status": "failed",
      "branch": "proj-102",
      "worktree": "./worktrees/proj-102",
      "started_at": "2026-03-01T14:30:23Z",
      "failed_at": "2026-03-01T14:33:12Z",
      "current_step": "implement",
      "error": "opencode exited with code 1: compilation error in src/main.rs",
      "steps_completed": []
    },
    "PROJ-103": {
      "status": "pending",
      "branch": null,
      "worktree": null
    }
  },
  "merge_queue": ["proj-101"],
  "merged": []
}
```

### 6.2 Ticket Lifecycle

| Status | Meaning | Transition |
|--------|---------|------------|
| `pending` | Ticket fetched but not yet started | Worker picks it up → `in_progress` |
| `in_progress` | Worker is actively processing | All steps pass → `completed`, any step fails → `failed` |
| `completed` | All steps finished successfully | Added to `merge_queue` |
| `failed` | A step failed; worktree preserved with diagnostics | User can resume or skip |
| `merged` | Successfully merged back to base branch | Terminal state |
| `skipped` | User chose to skip this ticket on resume | Terminal state |

### 6.3 Resume Behavior

When `queensland` is run and a `queensland.state.json` already exists, it enters resume mode. The resume logic is as follows:

- **Pending tickets** are picked up normally and processed from the beginning.
- **Completed tickets** are skipped (already in the merge queue).
- **Failed tickets** are presented to the user with their error context. The user chooses to retry (re-run from the failed step), restart (re-run from scratch), or skip.
- **In-progress tickets** (from a crash) are treated as failed. The worktree is inspected for partial work, and the diagnostic note includes what step was interrupted.
- **Merged tickets** are skipped entirely.

A fresh run can be forced with `queensland --new`, which archives the old state file and starts clean.

---

## 7. Prompt Templating

Prompts are stored as Markdown files with simple variable interpolation using double-brace syntax. Queensland supports two scopes for prompt resolution.

### 7.1 Resolution Order

1. **Local:** `./prompts/` in the project directory (highest priority)
2. **Global:** `~/.config/queensland/prompts/` (user-wide defaults)

Local prompts override global prompts of the same name. This lets you set up personal defaults (coding style, review criteria) and override them per-project.

### 7.2 Template Syntax

```markdown
# prompts/implement.md

You are implementing ticket {{ticket.key}}: {{ticket.summary}}

## Description
{{ticket.description}}

## Notes
{{ticket.notes}}

## Requirements
- Follow existing code style and patterns
- Write tests for new functionality
- Do not modify unrelated code
```

### 7.3 Usage in Lua

```lua
local prompt = ql.prompt("implement.md", {
    ticket = ticket,  -- the ticket object, fields accessed via dot notation
    extra_context = "This project uses Axum for HTTP routing",
})

ql.ai.run({
    tool = "opencode",
    cwd = worktree_dir,
    prompt = prompt,
})
```

---

## 8. Error Handling

Error handling follows the principle of preserving maximum context while allowing non-broken work to continue. When a step fails within a parallel worker, the following sequence occurs:

1. **Pause the failed worker.** The worker stops executing further steps for this ticket.
2. **Write diagnostics.** A `QUEENSLAND_ERROR.md` file is written to the worktree root containing: the ticket key and summary, which step failed, the full error output (stdout and stderr), a timestamp, and the command that was run.
3. **Update state.** The ticket status is set to `"failed"` in `queensland.state.json` with the error message.
4. **Continue other workers.** All non-failed workers continue to completion. The failure of one ticket does not affect others.
5. **Report at end.** After all workers finish, the summary shows which tickets succeeded and which failed, with pointers to the diagnostic files.

### 8.1 QUEENSLAND_ERROR.md Format

```markdown
# Queensland Error Report

**Ticket:** PROJ-102 - Implement user authentication
**Failed Step:** implement
**Timestamp:** 2026-03-01T14:33:12Z
**Command:** opencode "Implement ticket PROJ-102..."
**Exit Code:** 1

## stderr
```
error[E0433]: failed to resolve: use of undeclared crate
```

## stdout
```
(truncated output from the AI tool)
```
```

### 8.2 Lua-Level Error Handling

Users can implement custom error handling in their Lua templates using standard `pcall`/`xpcall` patterns. The `ql.ai.run` function returns a result object rather than throwing, so users can branch on success/failure:

```lua
local result = ql.ai.run({ tool = "opencode", cwd = dir, prompt = prompt })

if not result.success then
    ql.log("error", "Implementation failed: " .. result.stderr)
    -- Optionally retry with a different prompt or tool
    result = ql.ai.run({ tool = "claude", cwd = dir, prompt = fallback_prompt })
end

if not result.success then
    error("All implementation attempts failed")  -- triggers worker pause
end
```

---

## 9. Merge Strategy

Merging is sequential and is the one phase where interactive AI assistance is offered. The merge process works as follows:

1. Completed tickets are added to the merge queue in the order they finished.
2. Each branch is merged into the base branch one at a time using `git merge`.
3. If the merge succeeds cleanly, the worktree is removed and the ticket is marked as merged.
4. If the merge has conflicts, Queensland enters interactive conflict resolution mode.

### 9.1 Interactive Conflict Resolution

When a merge conflict occurs, Queensland presents the conflict to the user alongside an AI assistant. The AI can analyze the conflict, suggest resolutions, and apply them, but the user has final approval. This is the only place in the pipeline where Queensland intentionally opens a dialogue rather than running autonomously.

The conflict resolution session provides: a summary of what both branches changed, the AI tool's suggested resolution, the ability to accept, modify, or manually resolve, and the option to skip the merge and leave the branch for manual handling later.

---

## 10. CLI Interface

| Command | Description |
|---------|-------------|
| `queensland` | Run the pipeline (or resume if state file exists) |
| `queensland --new` | Start a fresh run, archiving any existing state |
| `queensland status` | Show the current state of all tickets in a run |
| `queensland resume` | Explicitly resume a paused/failed run |
| `queensland cleanup` | Remove all worktrees and state, restoring the repo to clean state |
| `queensland merge` | Run only the merge phase for completed tickets |
| `queensland inspect <ticket>` | Show detailed status and diagnostics for a specific ticket |

### 10.1 Configuration Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--template, -t` | `queensland.lua` | Path to the Lua template file |
| `--concurrency, -c` | `4` | Max parallel workers (overrides template setting) |
| `--dry-run` | `false` | Show what would be done without executing |
| `--verbose, -v` | `false` | Enable debug logging |
| `--worktree-dir` | `./worktrees` | Directory for git worktrees |

---

## 11. Project Structure

### 11.1 Queensland Crate Structure

```
queensland/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point (clap)
│   ├── engine.rs            # Core orchestration loop
│   ├── lua_runtime.rs       # Lua VM setup, API binding
│   ├── worker.rs            # Parallel worker implementation
│   ├── git.rs               # Git operations (worktree, merge)
│   ├── process.rs           # Process spawning and supervision
│   ├── state.rs             # State persistence and resume logic
│   ├── prompt.rs            # Prompt template loading and interpolation
│   ├── tui.rs               # Terminal progress display
│   └── conflict.rs          # Interactive merge conflict resolution
├── lua/
│   └── stdlib.lua           # Lua-side helpers (optional)
└── tests/
    ├── integration/         # End-to-end tests with git repos
    └── lua/                 # Lua template tests
```

### 11.2 User Project Layout

```
my-project/
├── queensland.lua           # Pipeline definition
├── prompts/
│   ├── implement.md         # Implementation prompt
│   ├── review.md            # Review prompt
│   └── fix.md               # Fix/retry prompt
├── queensland.state.json    # Auto-generated run state
├── worktrees/               # Auto-generated worktree directory
│   ├── proj-101/
│   ├── proj-102/
│   └── ...
├── src/                     # Actual project source
└── ...
```

---

## 12. Data Structures

Key Rust types that form the backbone of the system:

### 12.1 Core Types

```rust
/// Result of running an external process
struct ProcessResult {
    success: bool,
    stdout: String,
    stderr: String,
    exit_code: i32,
    duration: Duration,
}

/// A ticket as seen by the engine
struct Ticket {
    key: String,
    summary: String,
    fields: HashMap<String, serde_json::Value>,  // all other fields
}

/// Status of a single ticket in a run
enum TicketStatus {
    Pending,
    InProgress { step: String, started_at: DateTime },
    Completed { steps: Vec<String>, completed_at: DateTime },
    Failed { step: String, error: String, failed_at: DateTime },
    Merged { merged_at: DateTime },
    Skipped,
}

/// Overall run state
struct RunState {
    run_id: String,
    started_at: DateTime,
    base_branch: String,
    concurrency: usize,
    template_path: PathBuf,
    tickets: HashMap<String, TicketState>,
    merge_queue: Vec<String>,
    merged: Vec<String>,
}
```

### 12.2 Registered AI Tool

```rust
/// Configuration for a registered AI tool
struct AiToolConfig {
    name: String,
    bin: PathBuf,
    default_args: Vec<String>,
    prompt_flag: Option<String>,  // None = positional arg
    timeout: Duration,
}
```

---

## 13. Implementation Phases

### Phase 1: Foundation

The minimum viable tool that can run a parallel AI pipeline end-to-end.

- Rust binary with clap CLI
- Embedded Lua 5.4 via mlua
- `ql.exec()`, `ql.log()`, `ql.env()`
- `ql.git.worktree_add/remove`, `ql.git.commit`
- `ql.ai.register()` and `ql.ai.run()` with process spawning
- `ql.parallel()` with semaphore-based concurrency
- Basic state file write/read
- Simple terminal output (no TUI)

### Phase 2: Resilience

Make it survivable and resumable.

- Full state lifecycle (all ticket statuses, transitions)
- Resume mode with user prompts for failed tickets
- `QUEENSLAND_ERROR.md` generation
- Crash recovery (detecting interrupted `in_progress` tickets)
- `queensland cleanup` command
- `queensland status` command

### Phase 3: Merge and Prompts

Complete the pipeline with merge support and prompt templating.

- Sequential merge with conflict detection
- Interactive AI-assisted conflict resolution
- Prompt template loading with global/local scopes
- Variable interpolation in prompt templates

### Phase 4: Polish

Improve the experience for daily use.

- TUI with live worker status
- `--dry-run` mode
- `queensland inspect` command
- Worktree directory configuration
- Comprehensive error messages and suggestions

---

## 14. Open Questions

| # | Question | Context |
|---|----------|---------|
| 1 | Should Queensland support step-level resume within a ticket? | Currently, failed tickets restart from the failed step. Should we support restarting from an arbitrary step? Adds complexity but useful for long pipelines. |
| 2 | How should worktree naming handle key collisions? | If two runs produce the same ticket key, or a branch already exists. Append run ID? Fail fast? |
| 3 | Should the merge phase support non-interactive mode? | For CI/CD use cases, a `--no-interactive` flag that fails on conflict instead of opening a session. |
| 4 | Is Lua 5.4 sufficient or should we consider LuaJIT? | LuaJIT is faster but stuck on 5.1 semantics. For this use case, execution speed of Lua itself is irrelevant (all time is in subprocesses), so 5.4 features are probably more valuable. |
| 5 | Should prompts support conditionals or loops? | Current design is simple variable interpolation. If templates need logic, users can build the prompt string in Lua directly. Keeping templates dumb may be a feature. |
| 6 | What is the strategy for AI tool output capture during interactive use? | Some AI tools (Claude Code) are interactive. Should Queensland pipe through the terminal or capture silently? This may need to be per-tool configurable. |

---

## Appendix A: Example queensland.lua

A complete example template showing a typical workflow:

```lua
local ql = require("queensland")

-- Configuration
ql.concurrency = 4

-- Register AI tools
ql.ai.register("opencode", {
    bin = "opencode",
    default_args = {},
    timeout = 900,
})

ql.ai.register("claude", {
    bin = "claude",
    default_args = { "--dangerously-skip-permissions" },
    prompt_flag = "--prompt",
    timeout = 600,
})

-- Fetch tickets (user-defined)
local function fetch_tickets()
    local result = ql.exec(".", "bd", "ready", "--json")
    if not result.success then
        error("Failed to fetch tickets: " .. result.stderr)
    end
    return ql.json.decode(result.stdout)
end

-- Pipeline
local tickets = fetch_tickets()
ql.log("info", string.format("Found %d tickets", #tickets))

ql.parallel(tickets, function(ticket)
    local branch = ticket.key:lower():gsub("[^%w%-]", "-")
    local dir = ql.git.worktree_add(branch)

    -- Step 1: Implement
    local impl_prompt = ql.prompt("implement.md", { ticket = ticket })
    local impl = ql.ai.run({
        tool = "opencode",
        cwd = dir,
        prompt = impl_prompt,
    })
    if not impl.success then
        error("Implementation failed: " .. impl.stderr)
    end

    -- Step 2: Test
    local test = ql.exec(dir, "cargo", "test")
    if not test.success then
        error("Tests failed: " .. test.stderr)
    end

    -- Step 3: Review
    local review_prompt = ql.prompt("review.md", { ticket = ticket })
    local review = ql.ai.run({
        tool = "claude",
        cwd = dir,
        prompt = review_prompt,
    })
    if not review.success then
        error("Review failed: " .. review.stderr)
    end

    -- Step 4: Commit and push
    ql.git.commit(dir, string.format("%s: %s", ticket.key, ticket.summary))
    ql.git.push(dir, branch)
end)

-- Merge phase
ql.git.merge_all()
```
