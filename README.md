# grit

Coordination layer for parallel AI agents on top of git.

**Problem**: Running 20+ AI agents in parallel on a codebase creates O(N^2) merge conflicts. Git operates at the line level — when two agents edit different functions in the same file, git sees conflicting hunks.

**Solution**: grit locks at the **AST/function level** using tree-sitter. Different functions in the same file never conflict. Agents work in isolated git worktrees and merges are serialized automatically.

```
Raw Git with 20 agents:  84.5% merge failure rate
Grit with 20 agents:     0% merge failure rate
```

## Install

```bash
# From source
cargo install --git https://github.com/pszymkowiak/grit

# Or clone and build
git clone https://github.com/pszymkowiak/grit.git
cd grit
cargo build --release
# Binary at ./target/release/grit
```

## Quick Start

```bash
# In any git repo
cd your-project
grit init              # Parse AST, build symbol index

# Agent claims functions before editing
grit claim -a agent-1 -i "add validation" src/auth.ts::validateToken src/auth.ts::refreshToken

# Agent works in its isolated worktree at .grit/worktrees/agent-1/
# ... edit files ...

# Agent finishes — auto-commit, merge, release locks
grit done -a agent-1
```

## How It Works

1. **`grit init`** — Parses all source files with tree-sitter, indexes every function/method/class into a SQLite registry (`.grit/registry.db`)

2. **`grit claim`** — Agent reserves specific symbols (functions). If another agent already holds one, the claim is **blocked** and alternatives are suggested. Creates an isolated git worktree for the agent.

3. **`grit done`** — Commits changes in the worktree, merges back into main branch (serialized via file lock), removes worktree, releases all locks.

## Commands

```
grit init                          # Initialize grit in current repo
grit claim -a <agent> -i <intent> <symbols...>   # Claim symbols
grit done -a <agent>               # Merge + release
grit status                        # Show all active locks
grit symbols [--file <pattern>]    # List indexed symbols
grit plan <symbols...>             # Check availability without locking
grit release -a <agent> <symbols...>  # Release specific locks
grit gc                            # Clean up expired locks
grit heartbeat -a <agent>          # Refresh TTL on agent's locks
grit watch                         # Real-time event stream (Unix socket)
```

## Supported Languages

- TypeScript / JavaScript
- Rust
- Python

(Extensible via tree-sitter grammars)

## Architecture

```
┌─────────────────────────────────┐
│           your git repo         │
├─────────────────────────────────┤
│  .grit/                         │
│  ├── registry.db   (SQLite WAL) │  ← symbol index + lock table
│  ├── room.sock     (Unix)       │  ← real-time notifications
│  ├── merge.lock    (RAII)       │  ← serializes git merges
│  └── worktrees/                 │
│      ├── agent-1/  (git wt)    │  ← isolated working directory
│      ├── agent-2/  (git wt)    │
│      └── ...                    │
└─────────────────────────────────┘
```

## Benchmark

20 agents modifying different functions in the same files, 10 rounds:

| | Raw Git | Grit |
|---|---|---|
| Merges OK | 31 / 200 | 200 / 200 |
| Merges FAILED | 169 | 0 |
| Conflict files | 242 | 0 |
| Failure rate | 84.5% | 0% |
| Execution | sequential | parallel |

## Use with Claude Code

```bash
# Launch parallel agents that coordinate via grit
claude -p "You are agent-1. Use grit at $(which grit) to:
1. grit claim -a agent-1 -i 'add validation' src/auth.ts::validateToken
2. Edit the function in .grit/worktrees/agent-1/
3. grit done -a agent-1"
```

## License

MIT
