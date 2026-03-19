# grit

> **⚠️ EXPERIMENTAL — This project is in active testing. APIs and behavior may change without notice.**

**Git for AI agents — zero merge conflicts, any number of parallel agents, same codebase.**

> When 50 agents work on the same repo, git breaks. Grit doesn't.

Translations: [Francais](docs/README.fr.md) | [Deutsch](docs/README.de.md) | [Espanol](docs/README.es.md) | [Portugues](docs/README.pt.md) | [Italiano](docs/README.it.md) | [Nederlands](docs/README.nl.md) | [日本語](docs/README.ja.md) | [中文](docs/README.zh.md) | [한국어](docs/README.ko.md) | [Русский](docs/README.ru.md) | [العربية](docs/README.ar.md) | [हिन्दी](docs/README.hi.md)

---

## Benchmark Results (5 iterations x 5 rounds)

```
Agents │ Git Merge Failures │ Grit Merge Failures │ Git Conflict Files
───────┼────────────────────┼─────────────────────┼───────────────────
     1 │         0  (0%)    │         0  (0%)     │        0
     2 │         5  (50%)   │         0  (0%)     │       38
     5 │        20  (80%)   │         0  (0%)     │       80
    10 │        43  (86%)   │         0  (0%)     │       90
    20 │        83  (83%)   │         0  (0%)     │      130
    50 │       175  (70%)   │         0  (0%)     │      175
```

**Grit: 0 conflicts across all 1,500 merge attempts.**

## How It Works

```
                        THE PROBLEM
  ┌─────────────────────────────────────────────────┐
  │  10 AI agents edit different functions           │
  │  in the SAME file (auth.ts)                     │
  │                                                 │
  │  Git sees: same file changed on 10 branches     │
  │  Result:   O(N²) merge conflicts                │
  └─────────────────────────────────────────────────┘

                        THE SOLUTION
  ┌─────────────────────────────────────────────────┐
  │  Grit locks at the FUNCTION level (AST)         │
  │  not the FILE level (lines)                     │
  │                                                 │
  │  Agent-1 locks: validateToken()                 │
  │  Agent-2 locks: refreshToken()                  │
  │  → Same file, different functions, zero conflict│
  └─────────────────────────────────────────────────┘

  ┌──────────┐    ┌──────────┐    ┌──────────┐
  │ 1. CLAIM │───▶│ 2. WORK  │───▶│ 3. DONE  │
  │          │    │          │    │          │
  │ Lock AST │    │ Parallel │    │ Rebase + │
  │ symbols  │    │ worktrees│    │ Merge    │
  └──────────┘    └──────────┘    └──────────┘
       │               │               │
       ▼               ▼               ▼
  ┌──────────┐    ┌──────────┐    ┌──────────┐
  │ SQLite   │    │ .grit/   │    │ Serial   │
  │ or S3    │    │ worktrees│    │ file lock│
  │ lock DB  │    │ /agent-N │    │ → merge  │
  └──────────┘    └──────────┘    └──────────┘
```

## Architecture

```
┌─────────────────────────────────────────┐
│              your git repo              │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← symbol index + lock table
│  ├── config.json                        │  ← backend config (local/S3)
│  ├── room.sock      (Unix socket)       │  ← real-time event stream
│  ├── merge.lock     (RAII file lock)    │  ← serializes git merges
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← isolated working dir
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  Backends:                              │
│  ├── Local: SQLite WAL (default)        │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (self-hosted)                │
└─────────────────────────────────────────┘
```

---

## Problem

Running N AI agents in parallel on a codebase creates O(N²) merge conflicts. Git operates at the **line level** — when two agents edit different functions in the same file, git sees conflicting hunks and the merge fails.

## Solution

Grit locks at the **AST/function level** using tree-sitter. Each agent claims specific functions before editing. Different functions in the same file never conflict. Agents work in isolated git worktrees and merges are serialized automatically.

## Install

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## Quick Start

```bash
cd your-project
grit init                    # Parse AST, build symbol index

# Agent claims functions before editing
grit claim -a agent-1 -i "add validation" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# Agent works in isolated worktree: .grit/worktrees/agent-1/
# ... edit files ...

# Finish: auto-commit, rebase, merge, release locks
grit done -a agent-1
```

## Session Workflow (GitHub integration)

```bash
grit session start auth-refactor        # Create branch grit/auth-refactor
# ... agents claim, work, done ...
grit session pr                         # Push branch + create GitHub PR
grit session end                        # Cleanup locks, back to base branch
```

## Commands

```
grit init                                    # Initialize symbol index
grit claim -a <agent> -i <intent> <syms...>  # Lock symbols + create worktree
grit done  -a <agent>                        # Merge + release locks
grit status                                  # Show active locks
grit symbols [--file <pattern>]              # List indexed symbols
grit plan <symbols...>                       # Check availability (dry-run)
grit release -a <agent> <symbols...>         # Release specific locks
grit gc                                      # Clean expired locks
grit heartbeat -a <agent>                    # Refresh lock TTL
grit watch                                   # Real-time event stream
grit session start|status|pr|end             # Feature branch lifecycle
grit config set-s3|set-local|show            # Backend configuration
```

## Supported Languages

TypeScript, JavaScript, Rust, Python (extensible via tree-sitter grammars)

---

## License

MIT — Copyright (c) 2026 Patrick Szymkowiak
