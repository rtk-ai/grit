# grit

**Git para agentes IA — cero conflictos de merge, cualquier numero de agentes en paralelo, mismo codebase.**

> Cuando 50 agentes trabajan en el mismo repo, git falla. Grit no.

Read in English: [README.md](../README.md)

---

## Resultados benchmark (5 iteraciones x 5 rondas)

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

**Grit: 0 conflictos en los 1.500 intentos de merge.**

## Como funciona

```
                        EL PROBLEMA
  ┌─────────────────────────────────────────────────┐
  │  10 agentes IA editan funciones diferentes      │
  │  en el MISMO archivo (auth.ts)                  │
  │                                                 │
  │  Git ve: mismo archivo modificado en 10 ramas   │
  │  Resultado: O(N²) conflictos de merge           │
  └─────────────────────────────────────────────────┘

                        LA SOLUCION
  ┌─────────────────────────────────────────────────┐
  │  Grit bloquea a nivel de FUNCION (AST)          │
  │  no a nivel de ARCHIVO (lineas)                 │
  │                                                 │
  │  Agent-1 bloquea: validateToken()               │
  │  Agent-2 bloquea: refreshToken()                │
  │  → Mismo archivo, funciones diferentes, 0       │
  │    conflictos                                   │
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

## Arquitectura

```
┌─────────────────────────────────────────┐
│              tu repo git                │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← indice de simbolos + tabla de bloqueos
│  ├── config.json                        │  ← config backend (local/S3)
│  ├── room.sock      (Unix socket)       │  ← flujo de eventos en tiempo real
│  ├── merge.lock     (RAII file lock)    │  ← serializa merges git
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← directorio de trabajo aislado
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  Backends:                              │
│  ├── Local: SQLite WAL (por defecto)    │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (autoalojado)                │
└─────────────────────────────────────────┘
```

---

## Problema

Ejecutar N agentes IA en paralelo sobre un codebase crea conflictos de merge O(N²). Git opera a nivel de **lineas** — cuando dos agentes editan funciones distintas en el mismo archivo, git ve fragmentos en conflicto y el merge falla.

## Solucion

Grit bloquea a nivel **AST/funcion** usando tree-sitter. Cada agente reserva funciones especificas antes de editarlas. Funciones distintas en el mismo archivo nunca generan conflictos. Los agentes trabajan en worktrees git aislados y los merges se serializan automaticamente.

## Instalacion

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## Inicio rapido

```bash
cd tu-proyecto
grit init                    # Parsear AST, construir indice de simbolos

# El agente reserva funciones antes de editarlas
grit claim -a agent-1 -i "agregar validacion" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# El agente trabaja en worktree aislado: .grit/worktrees/agent-1/
# ... editar archivos ...

# Terminar: auto-commit, rebase, merge, liberar bloqueos
grit done -a agent-1
```

## Workflow de sesion (integracion GitHub)

```bash
grit session start auth-refactor        # Crear rama grit/auth-refactor
# ... agentes claim, trabajan, done ...
grit session pr                         # Push rama + crear PR en GitHub
grit session end                        # Limpiar bloqueos, volver a rama base
```

## Comandos

```
grit init                                    # Inicializar indice de simbolos
grit claim -a <agent> -i <intent> <syms...>  # Bloquear simbolos + crear worktree
grit done  -a <agent>                        # Merge + liberar bloqueos
grit status                                  # Mostrar bloqueos activos
grit symbols [--file <pattern>]              # Listar simbolos indexados
grit plan <symbols...>                       # Verificar disponibilidad (dry-run)
grit release -a <agent> <symbols...>         # Liberar bloqueos especificos
grit gc                                      # Limpiar bloqueos expirados
grit heartbeat -a <agent>                    # Refrescar TTL de bloqueos
grit watch                                   # Flujo de eventos en tiempo real
grit session start|status|pr|end             # Ciclo de vida de rama feature
grit config set-s3|set-local|show            # Configuracion del backend
```

## Lenguajes soportados

TypeScript, JavaScript, Rust, Python (extensible via gramaticas tree-sitter)

---

## Licencia

MIT — Copyright (c) 2026 Patrick Szymkowiak
