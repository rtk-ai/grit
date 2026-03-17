# grit

**Git voor AI-agents — nul merge-conflicten, onbeperkt aantal parallelle agents, dezelfde codebase.**

> Wanneer 50 agents aan dezelfde repo werken, breekt git. Grit niet.

Read in English: [README.md](../README.md)

---

## Benchmarkresultaten (5 iteraties x 5 rondes)

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

**Grit: 0 conflicten bij alle 1.500 merge-pogingen.**

## Hoe het werkt

```
                        HET PROBLEEM
  ┌─────────────────────────────────────────────────┐
  │  10 AI-agents bewerken verschillende functies   │
  │  in HETZELFDE bestand (auth.ts)                 │
  │                                                 │
  │  Git ziet: zelfde bestand gewijzigd op 10       │
  │  branches                                       │
  │  Resultaat: O(N²) merge-conflicten              │
  └─────────────────────────────────────────────────┘

                        DE OPLOSSING
  ┌─────────────────────────────────────────────────┐
  │  Grit vergrendelt op FUNCTIENIVEAU (AST)        │
  │  niet op BESTANDSNIVEAU (regels)                │
  │                                                 │
  │  Agent-1 vergrendelt: validateToken()           │
  │  Agent-2 vergrendelt: refreshToken()            │
  │  → Zelfde bestand, verschillende functies, 0    │
  │    conflicten                                   │
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
  │ of S3    │    │ worktrees│    │ file lock│
  │ lock DB  │    │ /agent-N │    │ → merge  │
  └──────────┘    └──────────┘    └──────────┘
```

## Architectuur

```
┌─────────────────────────────────────────┐
│              jouw git repo              │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← symboolindex + vergrendeltabel
│  ├── config.json                        │  ← backend-configuratie (lokaal/S3)
│  ├── room.sock      (Unix-socket)       │  ← realtime-evenementenstroom
│  ├── merge.lock     (RAII-bestandslock) │  ← serialiseert git-merges
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← geisoleerde werkdirectory
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  Backends:                              │
│  ├── Lokaal: SQLite WAL (standaard)     │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (zelf gehost)               │
└─────────────────────────────────────────┘
```

---

## Probleem

N AI-agents parallel op een codebase draaien veroorzaakt O(N²) merge-conflicten. Git werkt op **regelniveau** — wanneer twee agents verschillende functies in hetzelfde bestand bewerken, ziet git tegenstrijdige fragmenten en faalt de merge.

## Oplossing

Grit vergrendelt op **AST/functieniveau** met tree-sitter. Elke agent reserveert specifieke functies voor bewerking. Verschillende functies in hetzelfde bestand veroorzaken nooit conflicten. Agents werken in geisoleerde git worktrees en merges worden automatisch geserialiseerd.

## Installatie

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## Snelstart

```bash
cd jouw-project
grit init                    # AST parsen, symboolindex opbouwen

# Agent reserveert functies voor bewerking
grit claim -a agent-1 -i "validatie toevoegen" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# Agent werkt in geisoleerde worktree: .grit/worktrees/agent-1/
# ... bestanden bewerken ...

# Klaar: auto-commit, rebase, merge, vergrendelingen vrijgeven
grit done -a agent-1
```

## Sessie-workflow (GitHub-integratie)

```bash
grit session start auth-refactor        # Branch grit/auth-refactor aanmaken
# ... agents claim, werken, done ...
grit session pr                         # Branch pushen + GitHub-PR aanmaken
grit session end                        # Vergrendelingen opruimen, terug naar basisbranch
```

## Commando's

```
grit init                                    # Symboolindex initialiseren
grit claim -a <agent> -i <intent> <syms...>  # Symbolen vergrendelen + worktree aanmaken
grit done  -a <agent>                        # Merge + vergrendelingen vrijgeven
grit status                                  # Actieve vergrendelingen tonen
grit symbols [--file <pattern>]              # Geindexeerde symbolen weergeven
grit plan <symbols...>                       # Beschikbaarheid controleren (dry-run)
grit release -a <agent> <symbols...>         # Specifieke vergrendelingen vrijgeven
grit gc                                      # Verlopen vergrendelingen opruimen
grit heartbeat -a <agent>                    # Vergrendeling-TTL vernieuwen
grit watch                                   # Realtime-evenementenstroom
grit session start|status|pr|end             # Feature-branch-levenscyclus
grit config set-s3|set-local|show            # Backend-configuratie
```

## Ondersteunde talen

TypeScript, JavaScript, Rust, Python (uitbreidbaar via tree-sitter-grammatica's)

---

## Licentie

MIT — Copyright (c) 2026 Patrick Szymkowiak
