# grit

**Git para agentes IA — zero conflitos de merge, qualquer numero de agentes em paralelo, mesmo codebase.**

> Quando 50 agentes trabalham no mesmo repo, o git quebra. O Grit nao.

Read in English: [README.md](../README.md)

---

## Resultados benchmark (5 iteracoes x 5 rodadas)

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

**Grit: 0 conflitos em todas as 1.500 tentativas de merge.**

## Como funciona

```
                        O PROBLEMA
  ┌─────────────────────────────────────────────────┐
  │  10 agentes IA editam funcoes diferentes        │
  │  no MESMO arquivo (auth.ts)                     │
  │                                                 │
  │  Git ve: mesmo arquivo alterado em 10 branches  │
  │  Resultado: O(N²) conflitos de merge            │
  └─────────────────────────────────────────────────┘

                        A SOLUCAO
  ┌─────────────────────────────────────────────────┐
  │  Grit bloqueia no nivel de FUNCAO (AST)         │
  │  nao no nivel de ARQUIVO (linhas)               │
  │                                                 │
  │  Agent-1 bloqueia: validateToken()              │
  │  Agent-2 bloqueia: refreshToken()               │
  │  → Mesmo arquivo, funcoes diferentes, 0 conflito│
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

## Arquitetura

```
┌─────────────────────────────────────────┐
│              seu repo git               │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← indice de simbolos + tabela de bloqueios
│  ├── config.json                        │  ← config backend (local/S3)
│  ├── room.sock      (Unix socket)       │  ← fluxo de eventos em tempo real
│  ├── merge.lock     (RAII file lock)    │  ← serializa merges git
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← diretorio de trabalho isolado
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  Backends:                              │
│  ├── Local: SQLite WAL (padrao)         │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (auto-hospedado)             │
└─────────────────────────────────────────┘
```

---

## Problema

Executar N agentes IA em paralelo num codebase cria conflitos de merge O(N²). O Git opera ao nivel de **linhas** — quando dois agentes editam funcoes diferentes no mesmo arquivo, o git ve trechos conflitantes e o merge falha.

## Solucao

Grit bloqueia ao nivel **AST/funcao** usando tree-sitter. Cada agente reserva funcoes especificas antes de editar. Funcoes diferentes no mesmo arquivo nunca geram conflitos. Os agentes trabalham em worktrees git isoladas e os merges sao serializados automaticamente.

## Instalacao

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## Inicio rapido

```bash
cd seu-projeto
grit init                    # Parsear AST, construir indice de simbolos

# O agente reserva funcoes antes de editar
grit claim -a agent-1 -i "adicionar validacao" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# O agente trabalha em worktree isolada: .grit/worktrees/agent-1/
# ... editar arquivos ...

# Terminar: auto-commit, rebase, merge, liberar bloqueios
grit done -a agent-1
```

## Workflow de sessao (integracao GitHub)

```bash
grit session start auth-refactor        # Criar branch grit/auth-refactor
# ... agentes claim, trabalham, done ...
grit session pr                         # Push branch + criar PR no GitHub
grit session end                        # Limpar bloqueios, voltar ao branch base
```

## Comandos

```
grit init                                    # Inicializar indice de simbolos
grit claim -a <agent> -i <intent> <syms...>  # Bloquear simbolos + criar worktree
grit done  -a <agent>                        # Merge + liberar bloqueios
grit status                                  # Mostrar bloqueios ativos
grit symbols [--file <pattern>]              # Listar simbolos indexados
grit plan <symbols...>                       # Verificar disponibilidade (dry-run)
grit release -a <agent> <symbols...>         # Liberar bloqueios especificos
grit gc                                      # Limpar bloqueios expirados
grit heartbeat -a <agent>                    # Atualizar TTL dos bloqueios
grit watch                                   # Fluxo de eventos em tempo real
grit session start|status|pr|end             # Ciclo de vida do branch feature
grit config set-s3|set-local|show            # Configuracao do backend
```

## Linguagens suportadas

TypeScript, JavaScript, Rust, Python (extensivel via gramaticas tree-sitter)

---

## Licenca

MIT — Copyright (c) 2026 Patrick Szymkowiak
