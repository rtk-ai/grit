# grit

**Git per agenti IA — zero conflitti di merge, qualsiasi numero di agenti in parallelo, stesso codebase.**

> Quando 50 agenti lavorano sullo stesso repo, git si rompe. Grit no.

Read in English: [README.md](../README.md)

---

## Risultati benchmark (5 iterazioni x 5 round)

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

**Grit: 0 conflitti su tutti i 1.500 tentativi di merge.**

## Come funziona

```
                        IL PROBLEMA
  ┌─────────────────────────────────────────────────┐
  │  10 agenti IA modificano funzioni diverse       │
  │  nello STESSO file (auth.ts)                    │
  │                                                 │
  │  Git vede: stesso file modificato su 10 branch  │
  │  Risultato: O(N²) conflitti di merge            │
  └─────────────────────────────────────────────────┘

                        LA SOLUZIONE
  ┌─────────────────────────────────────────────────┐
  │  Grit blocca a livello di FUNZIONE (AST)        │
  │  non a livello di FILE (righe)                  │
  │                                                 │
  │  Agent-1 blocca: validateToken()                │
  │  Agent-2 blocca: refreshToken()                 │
  │  → Stesso file, funzioni diverse, 0 conflitti   │
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
  │ o S3     │    │ worktrees│    │ file lock│
  │ lock DB  │    │ /agent-N │    │ → merge  │
  └──────────┘    └──────────┘    └──────────┘
```

## Architettura

```
┌─────────────────────────────────────────┐
│              il tuo repo git            │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← indice simboli + tabella blocchi
│  ├── config.json                        │  ← config backend (locale/S3)
│  ├── room.sock      (Unix socket)       │  ← flusso eventi in tempo reale
│  ├── merge.lock     (RAII file lock)    │  ← serializza i merge git
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← directory di lavoro isolata
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  Backend:                               │
│  ├── Locale: SQLite WAL (predefinito)   │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (self-hosted)                │
└─────────────────────────────────────────┘
```

---

## Problema

Eseguire N agenti IA in parallelo su un codebase crea conflitti di merge O(N²). Git opera a livello di **righe** — quando due agenti modificano funzioni diverse nello stesso file, git vede frammenti in conflitto e il merge fallisce.

## Soluzione

Grit blocca a livello **AST/funzione** usando tree-sitter. Ogni agente riserva funzioni specifiche prima di modificarle. Funzioni diverse nello stesso file non creano mai conflitti. Gli agenti lavorano in worktree git isolate e i merge vengono serializzati automaticamente.

## Installazione

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## Avvio rapido

```bash
cd tuo-progetto
grit init                    # Parsare AST, costruire indice simboli

# L'agente riserva funzioni prima di modificarle
grit claim -a agent-1 -i "aggiungere validazione" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# L'agente lavora in worktree isolata: .grit/worktrees/agent-1/
# ... modificare i file ...

# Terminare: auto-commit, rebase, merge, rilascio blocchi
grit done -a agent-1
```

## Workflow sessione (integrazione GitHub)

```bash
grit session start auth-refactor        # Creare branch grit/auth-refactor
# ... agenti claim, lavorano, done ...
grit session pr                         # Push branch + creare PR GitHub
grit session end                        # Pulizia blocchi, ritorno al branch base
```

## Comandi

```
grit init                                    # Inizializzare indice simboli
grit claim -a <agent> -i <intent> <syms...>  # Bloccare simboli + creare worktree
grit done  -a <agent>                        # Merge + rilasciare blocchi
grit status                                  # Mostrare blocchi attivi
grit symbols [--file <pattern>]              # Elencare simboli indicizzati
grit plan <symbols...>                       # Verificare disponibilita (dry-run)
grit release -a <agent> <symbols...>         # Rilasciare blocchi specifici
grit gc                                      # Pulire blocchi scaduti
grit heartbeat -a <agent>                    # Aggiornare TTL blocchi
grit watch                                   # Flusso eventi in tempo reale
grit session start|status|pr|end             # Ciclo di vita branch feature
grit config set-s3|set-local|show            # Configurazione backend
```

## Linguaggi supportati

TypeScript, JavaScript, Rust, Python (estensibile tramite grammatiche tree-sitter)

---

## Licenza

MIT — Copyright (c) 2026 Patrick Szymkowiak
