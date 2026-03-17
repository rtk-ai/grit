# grit

**Git pour les agents IA — zero conflit de merge, nombre illimite d'agents en parallele, meme codebase.**

> Quand 50 agents travaillent sur le meme repo, git casse. Grit non.

Read in English: [README.md](../README.md)

---

## Resultats benchmark (5 iterations x 5 rounds)

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

**Grit : 0 conflit sur l'ensemble des 1 500 tentatives de merge.**

## Comment ca marche

```
                        LE PROBLEME
  ┌─────────────────────────────────────────────────┐
  │  10 agents IA modifient des fonctions           │
  │  differentes dans le MEME fichier (auth.ts)     │
  │                                                 │
  │  Git voit : meme fichier modifie sur 10 branches│
  │  Resultat : O(N²) conflits de merge             │
  └─────────────────────────────────────────────────┘

                        LA SOLUTION
  ┌─────────────────────────────────────────────────┐
  │  Grit verrouille au niveau FONCTION (AST)       │
  │  pas au niveau FICHIER (lignes)                 │
  │                                                 │
  │  Agent-1 verrouille : validateToken()           │
  │  Agent-2 verrouille : refreshToken()            │
  │  → Meme fichier, fonctions differentes, 0 conflit│
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
  │ ou S3    │    │ worktrees│    │ file lock│
  │ lock DB  │    │ /agent-N │    │ → merge  │
  └──────────┘    └──────────┘    └──────────┘
```

## Architecture

```
┌─────────────────────────────────────────┐
│              votre repo git             │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← index symboles + table verrous
│  ├── config.json                        │  ← config backend (local/S3)
│  ├── room.sock      (Unix socket)       │  ← flux evenements temps reel
│  ├── merge.lock     (RAII file lock)    │  ← serialise les merges git
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← repertoire de travail isole
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  Backends :                             │
│  ├── Local : SQLite WAL (defaut)        │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (auto-heberge)              │
└─────────────────────────────────────────┘
```

---

## Probleme

Lancer N agents IA en parallele sur un codebase cree des conflits de merge en O(N²). Git fonctionne au niveau des **lignes** — quand deux agents modifient des fonctions differentes dans le meme fichier, git voit des hunks en conflit et le merge echoue.

## Solution

Grit verrouille au niveau **AST/fonction** via tree-sitter. Chaque agent reserve des fonctions specifiques avant de les modifier. Des fonctions differentes dans le meme fichier ne creent jamais de conflit. Les agents travaillent dans des worktrees git isolees et les merges sont serialises automatiquement.

## Installation

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## Utilisation rapide

```bash
cd votre-projet
grit init                    # Parse l'AST, construit l'index des symboles

# L'agent reserve des fonctions avant de les modifier
grit claim -a agent-1 -i "ajouter validation" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# L'agent travaille dans sa worktree isolee: .grit/worktrees/agent-1/
# ... modifier les fichiers ...

# Terminer: auto-commit, rebase, merge, liberation des verrous
grit done -a agent-1
```

## Workflow session (integration GitHub)

```bash
grit session start auth-refactor        # Cree la branche grit/auth-refactor
# ... les agents claim, travaillent, done ...
grit session pr                         # Push la branche + cree une PR GitHub
grit session end                        # Nettoyage verrous, retour branche base
```

## Commandes

```
grit init                                    # Initialiser l'index des symboles
grit claim -a <agent> -i <intent> <syms...>  # Verrouiller symboles + creer worktree
grit done  -a <agent>                        # Merge + liberer les verrous
grit status                                  # Afficher les verrous actifs
grit symbols [--file <pattern>]              # Lister les symboles indexes
grit plan <symbols...>                       # Verifier disponibilite (dry-run)
grit release -a <agent> <symbols...>         # Liberer des verrous specifiques
grit gc                                      # Nettoyer les verrous expires
grit heartbeat -a <agent>                    # Rafraichir le TTL des verrous
grit watch                                   # Flux d'evenements temps reel
grit session start|status|pr|end             # Cycle de vie branche feature
grit config set-s3|set-local|show            # Configuration du backend
```

## Langages supportes

TypeScript, JavaScript, Rust, Python (extensible via grammaires tree-sitter)

---

## Licence

MIT — Copyright (c) 2026 Patrick Szymkowiak
