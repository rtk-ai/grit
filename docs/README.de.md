# grit

**Git fur KI-Agenten — null Merge-Konflikte, beliebig viele parallele Agenten, gleiche Codebasis.**

> Wenn 50 Agenten am selben Repo arbeiten, bricht Git zusammen. Grit nicht.

Read in English: [README.md](../README.md)

---

## Benchmark-Ergebnisse (5 Iterationen x 5 Runden)

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

**Grit: 0 Konflikte bei allen 1.500 Merge-Versuchen.**

## Wie es funktioniert

```
                        DAS PROBLEM
  ┌─────────────────────────────────────────────────┐
  │  10 KI-Agenten bearbeiten verschiedene          │
  │  Funktionen in DERSELBEN Datei (auth.ts)        │
  │                                                 │
  │  Git sieht: gleiche Datei auf 10 Branches       │
  │  Ergebnis:  O(N²) Merge-Konflikte              │
  └─────────────────────────────────────────────────┘

                        DIE LOSUNG
  ┌─────────────────────────────────────────────────┐
  │  Grit sperrt auf FUNKTIONSEBENE (AST)           │
  │  nicht auf DATEIEBENE (Zeilen)                  │
  │                                                 │
  │  Agent-1 sperrt: validateToken()                │
  │  Agent-2 sperrt: refreshToken()                 │
  │  → Gleiche Datei, verschiedene Funktionen, 0    │
  │    Konflikte                                    │
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
  │ oder S3  │    │ worktrees│    │ file lock│
  │ lock DB  │    │ /agent-N │    │ → merge  │
  └──────────┘    └──────────┘    └──────────┘
```

## Architektur

```
┌─────────────────────────────────────────┐
│              Ihr Git-Repo               │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← Symbol-Index + Sperrtabelle
│  ├── config.json                        │  ← Backend-Konfiguration (lokal/S3)
│  ├── room.sock      (Unix-Socket)       │  ← Echtzeit-Ereignisstrom
│  ├── merge.lock     (RAII-Dateisperre)  │  ← serialisiert Git-Merges
│  └── worktrees/                         │
│      ├── agent-1/   (Git-Worktree)      │  ← isoliertes Arbeitsverzeichnis
│      ├── agent-2/   (Git-Worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  Backends:                              │
│  ├── Lokal: SQLite WAL (Standard)       │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (selbst gehostet)            │
└─────────────────────────────────────────┘
```

---

## Problem

N KI-Agenten parallel auf einer Codebasis erzeugen O(N²) Merge-Konflikte. Git arbeitet auf **Zeilenebene** — wenn zwei Agenten verschiedene Funktionen in derselben Datei bearbeiten, erkennt Git widerspruchliche Abschnitte und der Merge schlagt fehl.

## Losung

Grit sperrt auf **AST/Funktionsebene** mit tree-sitter. Jeder Agent reserviert bestimmte Funktionen vor der Bearbeitung. Verschiedene Funktionen in derselben Datei erzeugen nie Konflikte. Agenten arbeiten in isolierten Git-Worktrees und Merges werden automatisch serialisiert.

## Installation

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## Schnellstart

```bash
cd ihr-projekt
grit init                    # AST parsen, Symbol-Index aufbauen

# Agent reserviert Funktionen vor der Bearbeitung
grit claim -a agent-1 -i "Validierung hinzufuegen" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# Agent arbeitet in isoliertem Worktree: .grit/worktrees/agent-1/
# ... Dateien bearbeiten ...

# Fertig: Auto-Commit, Rebase, Merge, Sperren freigeben
grit done -a agent-1
```

## Session-Workflow (GitHub-Integration)

```bash
grit session start auth-refactor        # Branch grit/auth-refactor erstellen
# ... Agenten claim, arbeiten, done ...
grit session pr                         # Branch pushen + GitHub-PR erstellen
grit session end                        # Sperren bereinigen, zuruck zum Basisbranch
```

## Befehle

```
grit init                                    # Symbol-Index initialisieren
grit claim -a <agent> -i <intent> <syms...>  # Symbole sperren + Worktree erstellen
grit done  -a <agent>                        # Merge + Sperren freigeben
grit status                                  # Aktive Sperren anzeigen
grit symbols [--file <pattern>]              # Indexierte Symbole auflisten
grit plan <symbols...>                       # Verfugbarkeit prufen (Dry-Run)
grit release -a <agent> <symbols...>         # Bestimmte Sperren freigeben
grit gc                                      # Abgelaufene Sperren bereinigen
grit heartbeat -a <agent>                    # Sperr-TTL aktualisieren
grit watch                                   # Echtzeit-Ereignisstrom
grit session start|status|pr|end             # Feature-Branch-Lebenszyklus
grit config set-s3|set-local|show            # Backend-Konfiguration
```

## Unterstutzte Sprachen

TypeScript, JavaScript, Rust, Python (erweiterbar uber tree-sitter-Grammatiken)

---

## Lizenz

MIT — Copyright (c) 2026 Patrick Szymkowiak
