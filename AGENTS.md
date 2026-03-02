# Taskwarrior Workflow Context

> **Shell Function**: `tw` auto-sets `project:` to current git repo name.
> Use `tw` instead of `task` for all commands.

# 🚨 SESSION CLOSE PROTOCOL 🚨

**CRITICAL**: Before saying "done" or "complete", you MUST run this checklist:
```
[ ] 1. git status              (check what changed)
[ ] 2. git add <files>         (stage code changes)
[ ] 3. git commit -m "..."     (commit code)
[ ] 4. git push                (push to remote)
```

**NEVER skip this.** Work is not done until pushed.

## Core Rules
- **Default**: Use `tw` for ALL task tracking
- **Prohibited**: Do NOT use TodoWrite, TaskCreate, or markdown files for task tracking
- **Workflow**: Create task BEFORE writing code, mark started when beginning
- Git workflow: commit and push code at session end
- Session management: check `tw ready` for available work

## Essential Commands

### Finding Work
- `tw ready` - Show tasks ready to work (unblocked, by urgency)
- `tw status:pending` - All open tasks
- `tw +ACTIVE` - Your active work (started tasks)
- `tw <id> info` - Detailed task view with dependencies

### Creating & Updating
- `tw add "Summary of this task" priority:M` - New task
  - Priority: H (high), M (medium), L (low), or omit for none
  - Tags: `tw add "Fix login bug" +bug priority:H`
  - Types via tags: `+task`, `+bug`, `+feature`
- `tw <id> start` - Claim work (marks active)
- `tw <id> modify assignee:username` - Assign to someone (UDA)
- `tw <id> modify "new description"` - Update description
- `tw <id> annotate "additional notes"` - Add notes
- `tw <id> done` - Mark complete
- `tw <id1> <id2> <id3> done` - Complete multiple tasks at once
- `tw <id> done rc.confirmation:off` - Skip confirmation
- **WARNING**: Do NOT use `tw <id> edit` - it opens $EDITOR which blocks agents

### Dependencies & Blocking
- `tw <id> modify depends:<other-id>` - Add dependency
- `tw +BLOCKED` - Show all blocked tasks
- `tw +BLOCKING` - Show all tasks that block others
- `tw <id> info` - See dependencies and what's blocked

### Sync & Collaboration
- `task sync` - Sync with Taskserver (if configured)
- `tw /search query/` - Search tasks by keyword

### Project Health
- `tw summary` - Project statistics
- `tw burndown.daily` - Burndown chart
- `tw stats` - Overall statistics

## Common Workflows

**Starting work:**
```bash
tw ready                # Find available work
tw <id> info            # Review task details
tw <id> start           # Claim it
```

**Completing work:**
```bash
tw <id1> <id2> done             # Close completed tasks
git add . && git commit -m "..."  # Commit code changes
git push                         # Push to remote
```

**Creating dependent work:**
```bash
tw add "Implement feature X" +feature priority:M
# note the ID returned, e.g. 42
tw add "Write tests for X" +task priority:M depends:42
```
