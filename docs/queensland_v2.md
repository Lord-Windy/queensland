# Queensland v2: Rust + Lua Orchestration Engine

## Context

Queensland orchestrates AI coding agents (Claude Code, OpenCode, etc.) working on beads tickets in parallel via git worktrees and tmux. The original TRD defined a rigid TOML-configured pipeline. This redesign replaces the config with **Lua scripts** — the user writes a `queensland.lua` that IS the workflow. Rust provides primitives (get tickets, create worktrees, launch agents, wait), Lua composes them. This means linear pipelines, conditional logic, and fan-out/fan-in all come for free without Rust changes.

## Architecture

```
queensland run [script.lua]
    |
    v
  Rust: parse CLI, create mlua::Lua VM, register qld.* functions
    |
    v
  Rust: load and execute user's Lua script
    |
    v
  Lua script calls qld.tickets(), qld.worktree_create(), qld.agent_run(), etc.
    |
    v
  Each qld.* function calls into existing Rust modules (beads, worktree, tmux, agent)
    |
    v
  qld.parallel_wait() uses std::thread::scope to poll multiple tmux windows concurrently
    |
    v
  Script returns; queensland exits
```

## Lua API Surface

All functions live under a global `qld` table.

| Function | Returns | Notes |
|---|---|---|
| `qld.tickets()` | `[{id, title, description, priority, type}]` | Runs `bd ready` |
| `qld.ticket(id)` | single ticket table | Runs `bd show <id>` |
| `qld.worktree_create(ticket, opts?)` | `{path, branch, created}` | Wraps existing worktree.rs |
| `qld.worktree_remove(id)` | nil | git worktree remove + branch delete |
| `qld.worktree_list()` | `[{ticket_id, path, branch}]` | Lists .qld-worktrees/ |
| `qld.tmux_window(name, cwd)` | window name | Creates tmux window |
| `qld.tmux_has_window(name)` | boolean | Check existence |
| `qld.tmux_send(window, keys)` | nil | Non-blocking send-keys |
| `qld.tmux_wait(window)` | exit_code (int) | Blocking poll until pane exits |
| `qld.tmux_kill(window)` | nil | Kill window |
| `qld.tmux_list()` | `[string]` | List window names |
| `qld.agent_cmd(opts)` | string | Build command string (pure) |
| `qld.agent_run(ticket, wt, opts)` | window name | tmux_window + tmux_send combined |
| `qld.parallel_wait(windows)` | `[{window, exit_code}]` | Wait for N windows concurrently |
| `qld.exec(cmd, args, opts?)` | `{stdout, stderr, exit_code}` | Run any subprocess |
| `qld.log(level, msg)` | nil | tracing integration |
| `qld.sleep(secs)` | nil | Blocking sleep |
| `qld.repo_root()` | string | git rev-parse --show-toplevel |
| `qld.env(name)` | string or nil | Read env var |

## Example User Script

```lua
-- queensland.lua
local tickets = qld.tickets()
qld.log("info", "Found " .. #tickets .. " ready tickets")

local agent_opts = {
    agent = "opencode",
    args = {},
    suffix = "Do not commit.",
}

-- Launch all agents
local windows = {}
for _, ticket in ipairs(tickets) do
    local wt = qld.worktree_create(ticket)
    local w = qld.agent_run(ticket, wt, agent_opts)
    table.insert(windows, w)
end

-- Wait for all to finish
local results = qld.parallel_wait(windows)

-- Then run review pass
for i, ticket in ipairs(tickets) do
    if results[i].exit_code == 0 then
        local wt_path = qld.repo_root() .. "/.qld-worktrees/" .. ticket.id
        qld.agent_run(ticket, { path = wt_path }, {
            agent = "claude",
            args = {"--dangerously-skip-permissions"},
            prompt = "Review the changes in this worktree for " .. ticket.title,
            suffix = "Do not commit.",
        })
    end
end
```

## Concurrency Design

Lua is single-threaded. The parallelism lives in Rust:

- `qld.parallel_wait(windows)` takes an array of tmux window names
- Rust spawns threads via `std::thread::scope` (stdlib, no async runtime)
- Each thread polls its tmux window with `tmux list-panes -t <win> -F '#{pane_dead}'` every second
- When pane dies, reads `#{pane_dead_status}` for exit code
- All results collected and returned as a Lua table

For batching (e.g., 4 at a time), the user just does it in Lua:

```lua
local batch_size = 4
for i = 1, #tickets, batch_size do
    local batch = {}
    for j = i, math.min(i + batch_size - 1, #tickets) do
        -- create worktree + launch agent
        table.insert(batch, window_name)
    end
    qld.parallel_wait(batch)  -- blocks until batch done
end
```

## Changes to Existing Code

### Keep and adapt
- **`src/beads.rs`** — Add `#[derive(serde::Serialize)]` to Ticket, Priority, TicketType. Add `pub fn show(id: &str)`. Existing tests stay.
- **`src/worktree.rs`** — Make `create_one` pub. Add `remove()` and `list()` functions. Add `WorktreeInfo` struct with Serialize. Existing tests stay.
- **`src/error.rs`** — Add `LuaScript` and `TmuxCommand` variants. Add `impl From<mlua::Error>`.

### Rewrite from stubs
- **`src/main.rs`** — Clap CLI: `queensland run [script]`, `queensland clean`. Create Lua VM, register API, execute script.
- **`src/tmux.rs`** — 6 functions wrapping tmux CLI subprocesses. `wait_for_exit` polls pane_dead.
- **`src/agent.rs`** — `AgentOpts` struct, `build_command()` for string construction, `run_in_tmux()` combining tmux_window + tmux_send.

### New modules
- **`src/lua_api.rs`** — Bridge module. `pub fn register(lua: &Lua, ctx: &RepoContext)` registers all `qld.*` functions.
- **`src/parallel.rs`** — `pub fn parallel_wait(windows: Vec<String>) -> Vec<WaitResult>` using std::thread::scope.
- **`src/context.rs`** — `RepoContext { root, worktree_dir, branch_prefix }` with defaults. Replaces config.rs.

### Remove
- **`src/config.rs`** — Replaced by Lua. Delete the stub.
- **`toml` dep** — Remove from Cargo.toml.

## Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
mlua = { version = "0.11", features = ["lua54", "vendored", "send", "serde"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
```

## Implementation Phases

### Phase 1: Lua VM skeleton
1. Update Cargo.toml (add mlua, remove toml)
2. Create `src/context.rs` with RepoContext
3. Create `src/lua_api.rs` with register() — start with just `qld.log`, `qld.repo_root`, `qld.env`
4. Rewrite `src/main.rs` — clap CLI + Lua VM setup + script execution
5. Delete `src/config.rs`
6. Verify: write a test `queensland.lua` that logs a message

### Phase 2: Wire existing modules to Lua
7. Add Serialize derives to beads.rs types
8. Register `qld.tickets()` in lua_api.rs
9. Make worktree::create_one pub, add WorktreeInfo
10. Register `qld.worktree_create()` and `qld.worktree_list()`
11. Verify: Lua script that fetches tickets and creates worktrees

### Phase 3: Tmux + Agent
12. Implement tmux.rs (create_window, has_window, send_keys, wait_for_exit, kill_window, list_windows)
13. Implement agent.rs (AgentOpts, build_command, run_in_tmux)
14. Register tmux and agent functions in lua_api.rs
15. Verify: Lua script that launches one agent in a tmux window

### Phase 4: Concurrency
16. Implement parallel.rs with parallel_wait
17. Register `qld.parallel_wait()` in lua_api.rs
18. Verify: launch 2+ agents, wait for all

### Phase 5: Polish
19. Register `qld.exec()`, `qld.sleep()`
20. Add `qld.worktree_remove()` + `queensland clean` command
21. Error formatting (Lua stack traces with file/line)
22. Write the new TRD (docs/trd.md) reflecting this design

## Verification

- `cargo test` — all existing beads/worktree tests still pass
- `cargo build` — compiles clean
- Create a simple `queensland.lua` that calls `qld.tickets()` and `qld.log()`, run `queensland run`
- In a tmux session, run a full flow: tickets -> worktrees -> agents -> parallel_wait
- `queensland clean` removes worktrees
