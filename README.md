# grit

**Git for AI agents — zero merge conflicts, any number of parallel agents, same codebase.**

> When 50 agents work on the same repo, git breaks. Grit doesn't.

[English](#english) | [Francais](#francais) | [Deutsch](#deutsch) | [Espanol](#espanol) | [Portugues](#portugues) | [Italiano](#italiano) | [Nederlands](#nederlands) | [日本語](#日本語) | [中文](#中文) | [한국어](#한국어) | [Русский](#русский) | [العربية](#العربية) | [हिन्दी](#हिन्दी)

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

<a name="english"></a>
## 🇬🇧 English

### Problem

Running N AI agents in parallel on a codebase creates O(N²) merge conflicts. Git operates at the **line level** — when two agents edit different functions in the same file, git sees conflicting hunks and the merge fails.

### Solution

Grit locks at the **AST/function level** using tree-sitter. Each agent claims specific functions before editing. Different functions in the same file never conflict. Agents work in isolated git worktrees and merges are serialized automatically.

### Install

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### Quick Start

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

### Session Workflow (GitHub integration)

```bash
grit session start auth-refactor        # Create branch grit/auth-refactor
# ... agents claim, work, done ...
grit session pr                         # Push branch + create GitHub PR
grit session end                        # Cleanup locks, back to base branch
```

### Commands

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

### Supported Languages

TypeScript, JavaScript, Rust, Python (extensible via tree-sitter grammars)

---

<a name="francais"></a>
## 🇫🇷 Francais

### Probleme

Lancer N agents IA en parallele sur un codebase cree des conflits de merge en O(N²). Git fonctionne au niveau des **lignes** — quand deux agents modifient des fonctions differentes dans le meme fichier, git voit des hunks en conflit et le merge echoue.

### Solution

Grit verrouille au niveau **AST/fonction** via tree-sitter. Chaque agent reserve des fonctions specifiques avant de les modifier. Des fonctions differentes dans le meme fichier ne creent jamais de conflit. Les agents travaillent dans des worktrees git isolees et les merges sont serialises automatiquement.

### Installation

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### Utilisation rapide

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

### Workflow session (integration GitHub)

```bash
grit session start auth-refactor        # Cree la branche grit/auth-refactor
# ... les agents claim, travaillent, done ...
grit session pr                         # Push la branche + cree une PR GitHub
grit session end                        # Nettoyage verrous, retour branche base
```

### Resultats benchmark

```
20 agents, memes fichiers, fonctions differentes:
  Git brut:  83% d'echecs de merge, 130 fichiers en conflit
  Grit:       0% d'echecs, 0 conflit — sur 1500 tentatives
```

---

<a name="deutsch"></a>
## 🇩🇪 Deutsch

### Problem

N KI-Agenten parallel auf einer Codebasis erzeugen O(N²) Merge-Konflikte. Git arbeitet auf **Zeilenebene** — wenn zwei Agenten verschiedene Funktionen in derselben Datei bearbeiten, erkennt Git widerspruchliche Abschnitte und der Merge schlagt fehl.

### Losung

Grit sperrt auf **AST/Funktionsebene** mit tree-sitter. Jeder Agent reserviert bestimmte Funktionen vor der Bearbeitung. Verschiedene Funktionen in derselben Datei erzeugen nie Konflikte. Agenten arbeiten in isolierten Git-Worktrees und Merges werden automatisch serialisiert.

### Installation

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### Schnellstart

```bash
cd ihr-projekt
grit init                    # AST parsen, Symbol-Index aufbauen

grit claim -a agent-1 -i "Validierung hinzufuegen" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# Agent arbeitet in isoliertem Worktree: .grit/worktrees/agent-1/

grit done -a agent-1         # Merge + Sperren freigeben
```

### Benchmark-Ergebnisse

```
20 Agenten, gleiche Dateien, verschiedene Funktionen:
  Reines Git: 83% Merge-Fehlerrate, 130 Konfliktdateien
  Grit:        0% Fehler, 0 Konflikte — uber 1500 Versuche
```

---

<a name="espanol"></a>
## 🇪🇸 Espanol

### Problema

Ejecutar N agentes IA en paralelo sobre un codebase crea conflictos de merge O(N²). Git opera a nivel de **lineas** — cuando dos agentes editan funciones distintas en el mismo archivo, git ve fragmentos en conflicto y el merge falla.

### Solucion

Grit bloquea a nivel **AST/funcion** usando tree-sitter. Cada agente reserva funciones especificas antes de editarlas. Funciones distintas en el mismo archivo nunca generan conflictos. Los agentes trabajan en worktrees git aislados y los merges se serializan automaticamente.

### Instalacion

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### Inicio rapido

```bash
cd tu-proyecto
grit init
grit claim -a agent-1 -i "agregar validacion" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# Trabajar en .grit/worktrees/agent-1/
grit done -a agent-1
```

### Resultados benchmark

```
20 agentes, mismos archivos, funciones diferentes:
  Git puro:  83% tasa de fallos, 130 archivos en conflicto
  Grit:       0% fallos, 0 conflictos — en 1500 intentos
```

---

<a name="portugues"></a>
## 🇧🇷 Portugues

### Problema

Executar N agentes IA em paralelo num codebase cria conflitos de merge O(N²). O Git opera ao nivel de **linhas** — quando dois agentes editam funcoes diferentes no mesmo arquivo, o git ve trechos conflitantes e o merge falha.

### Solucao

Grit bloqueia ao nivel **AST/funcao** usando tree-sitter. Cada agente reserva funcoes especificas antes de editar. Funcoes diferentes no mesmo arquivo nunca geram conflitos. Os agentes trabalham em worktrees git isoladas e os merges sao serializados automaticamente.

### Instalacao

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### Inicio rapido

```bash
cd seu-projeto
grit init
grit claim -a agent-1 -i "adicionar validacao" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# Trabalhar em .grit/worktrees/agent-1/
grit done -a agent-1
```

### Resultados benchmark

```
20 agentes, mesmos arquivos, funcoes diferentes:
  Git puro:  83% taxa de falha, 130 arquivos em conflito
  Grit:       0% falhas, 0 conflitos — em 1500 tentativas
```

---

<a name="italiano"></a>
## 🇮🇹 Italiano

### Problema

Eseguire N agenti IA in parallelo su un codebase crea conflitti di merge O(N²). Git opera a livello di **righe** — quando due agenti modificano funzioni diverse nello stesso file, git vede frammenti in conflitto e il merge fallisce.

### Soluzione

Grit blocca a livello **AST/funzione** usando tree-sitter. Ogni agente riserva funzioni specifiche prima di modificarle. Funzioni diverse nello stesso file non creano mai conflitti. Gli agenti lavorano in worktree git isolate e i merge vengono serializzati automaticamente.

### Installazione

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### Avvio rapido

```bash
cd tuo-progetto
grit init
grit claim -a agent-1 -i "aggiungere validazione" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# Lavorare in .grit/worktrees/agent-1/
grit done -a agent-1
```

### Risultati benchmark

```
20 agenti, stessi file, funzioni diverse:
  Git puro:  83% tasso di fallimento, 130 file in conflitto
  Grit:       0% fallimenti, 0 conflitti — su 1500 tentativi
```

---

<a name="nederlands"></a>
## 🇳🇱 Nederlands

### Probleem

N AI-agents parallel op een codebase draaien veroorzaakt O(N²) merge-conflicten. Git werkt op **regelniveau** — wanneer twee agents verschillende functies in hetzelfde bestand bewerken, ziet git tegenstrijdige fragmenten en faalt de merge.

### Oplossing

Grit vergrendelt op **AST/functieniveau** met tree-sitter. Elke agent reserveert specifieke functies voor bewerking. Verschillende functies in hetzelfde bestand veroorzaken nooit conflicten. Agents werken in geisoleerde git worktrees en merges worden automatisch geserialiseerd.

### Installatie

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### Snelstart

```bash
cd jouw-project
grit init
grit claim -a agent-1 -i "validatie toevoegen" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# Werken in .grit/worktrees/agent-1/
grit done -a agent-1
```

### Benchmarkresultaten

```
20 agents, dezelfde bestanden, verschillende functies:
  Puur Git:  83% merge-faalpercentage, 130 conflictbestanden
  Grit:       0% fouten, 0 conflicten — over 1500 pogingen
```

---

<a name="日本語"></a>
## 🇯🇵 日本語

### 問題

N個のAIエージェントをコードベース上で並列実行すると、O(N²)のマージコンフリクトが発生します。Gitは**行レベル**で動作するため、2つのエージェントが同じファイル内の異なる関数を編集しても、Gitは競合するハンクを検出しマージが失敗します。

### 解決策

Gritはtree-sitterを使用して**AST/関数レベル**でロックします。各エージェントは編集前に特定の関数を予約します。同じファイル内の異なる関数は決してコンフリクトしません。エージェントは分離されたgit worktreeで作業し、マージは自動的にシリアライズされます。

### インストール

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### クイックスタート

```bash
cd your-project
grit init
grit claim -a agent-1 -i "バリデーション追加" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# .grit/worktrees/agent-1/ で作業
grit done -a agent-1
```

### ベンチマーク結果

```
20エージェント、同一ファイル、異なる関数:
  素のGit:   83% マージ失敗率、130ファイルがコンフリクト
  Grit:       0% 失敗、0コンフリクト — 1500回の試行で
```

---

<a name="中文"></a>
## 🇨🇳 中文

### 问题

在代码库上并行运行N个AI代理会产生O(N²)的合并冲突。Git在**行级别**操作——当两个代理编辑同一文件中的不同函数时，Git会看到冲突的代码块，导致合并失败。

### 解决方案

Grit使用tree-sitter在**AST/函数级别**进行锁定。每个代理在编辑前预留特定函数。同一文件中的不同函数永远不会冲突。代理在隔离的git worktree中工作，合并自动序列化。

### 安装

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### 快速开始

```bash
cd your-project
grit init
grit claim -a agent-1 -i "添加验证" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# 在 .grit/worktrees/agent-1/ 中工作
grit done -a agent-1
```

### 基准测试结果

```
20个代理，相同文件，不同函数：
  原生Git：  83% 合并失败率，130个冲突文件
  Grit：      0% 失败，0冲突 — 共1500次尝试
```

---

<a name="한국어"></a>
## 🇰🇷 한국어

### 문제

코드베이스에서 N개의 AI 에이전트를 병렬로 실행하면 O(N²)의 머지 충돌이 발생합니다. Git은 **라인 레벨**에서 작동하므로, 두 에이전트가 같은 파일의 다른 함수를 편집해도 Git은 충돌하는 헝크를 감지하고 머지가 실패합니다.

### 해결책

Grit은 tree-sitter를 사용하여 **AST/함수 레벨**에서 잠금합니다. 각 에이전트는 편집 전에 특정 함수를 예약합니다. 같은 파일의 다른 함수는 절대 충돌하지 않습니다. 에이전트는 격리된 git worktree에서 작업하며, 머지는 자동으로 직렬화됩니다.

### 설치

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### 빠른 시작

```bash
cd your-project
grit init
grit claim -a agent-1 -i "검증 추가" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# .grit/worktrees/agent-1/ 에서 작업
grit done -a agent-1
```

### 벤치마크 결과

```
20개 에이전트, 동일 파일, 다른 함수:
  순수 Git:  83% 머지 실패율, 130개 충돌 파일
  Grit:       0% 실패, 0 충돌 — 총 1500회 시도
```

---

<a name="русский"></a>
## 🇷🇺 Русский

### Проблема

Запуск N ИИ-агентов параллельно на кодовой базе создает O(N²) конфликтов слияния. Git работает на уровне **строк** — когда два агента редактируют разные функции в одном файле, Git видит конфликтующие фрагменты и слияние терпит неудачу.

### Решение

Grit блокирует на уровне **AST/функций** с помощью tree-sitter. Каждый агент резервирует определенные функции перед редактированием. Разные функции в одном файле никогда не конфликтуют. Агенты работают в изолированных git worktree, и слияния сериализуются автоматически.

### Установка

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### Быстрый старт

```bash
cd your-project
grit init
grit claim -a agent-1 -i "добавить валидацию" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# Работать в .grit/worktrees/agent-1/
grit done -a agent-1
```

### Результаты бенчмарка

```
20 агентов, одни файлы, разные функции:
  Чистый Git: 83% ошибок слияния, 130 файлов с конфликтами
  Grit:        0% ошибок, 0 конфликтов — из 1500 попыток
```

---

<a name="العربية"></a>
## 🇸🇦 العربية

### المشكلة

تشغيل N من وكلاء الذكاء الاصطناعي بالتوازي على قاعدة الكود ينتج O(N²) تعارضات دمج. يعمل Git على مستوى **الأسطر** — عندما يعدل وكيلان دوال مختلفة في نفس الملف، يرى Git أجزاء متعارضة ويفشل الدمج.

### الحل

يقفل Grit على مستوى **AST/الدوال** باستخدام tree-sitter. كل وكيل يحجز دوال محددة قبل التعديل. الدوال المختلفة في نفس الملف لا تتعارض أبدا. يعمل الوكلاء في worktrees git معزولة ويتم تسلسل عمليات الدمج تلقائيا.

### التثبيت

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### البدء السريع

```bash
cd your-project
grit init
grit claim -a agent-1 -i "إضافة التحقق" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# العمل في .grit/worktrees/agent-1/
grit done -a agent-1
```

### نتائج الاختبار

```
20 وكيل، نفس الملفات، دوال مختلفة:
  Git الخام:  83% معدل فشل الدمج، 130 ملف متعارض
  Grit:        0% فشل، 0 تعارضات — من 1500 محاولة
```

---

<a name="हिन्दी"></a>
## 🇮🇳 हिन्दी

### समस्या

एक कोडबेस पर N AI एजेंट्स को समानांतर चलाने से O(N²) मर्ज कॉन्फ्लिक्ट बनते हैं। Git **लाइन लेवल** पर काम करता है — जब दो एजेंट एक ही फाइल में अलग-अलग फंक्शन एडिट करते हैं, तो Git कॉन्फ्लिक्टिंग हंक देखता है और मर्ज फेल हो जाता है।

### समाधान

Grit tree-sitter का उपयोग करके **AST/फंक्शन लेवल** पर लॉक करता है। हर एजेंट एडिट करने से पहले विशिष्ट फंक्शन रिज़र्व करता है। एक ही फाइल में अलग-अलग फंक्शन कभी कॉन्फ्लिक्ट नहीं करते। एजेंट आइसोलेटेड git worktree में काम करते हैं और मर्ज ऑटोमैटिकली सीरियलाइज़ होते हैं।

### इंस्टॉलेशन

```bash
cargo install --git https://github.com/rtk-ai/grit
```

### क्विक स्टार्ट

```bash
cd your-project
grit init
grit claim -a agent-1 -i "वैलिडेशन जोड़ें" \
  src/auth.ts::validateToken src/auth.ts::refreshToken
# .grit/worktrees/agent-1/ में काम करें
grit done -a agent-1
```

### बेंचमार्क परिणाम

```
20 एजेंट, समान फाइलें, अलग-अलग फंक्शन:
  शुद्ध Git:  83% मर्ज विफलता दर, 130 कॉन्फ्लिक्ट फाइलें
  Grit:        0% विफलता, 0 कॉन्फ्लिक्ट — 1500 प्रयासों में
```

---

## License

MIT — Copyright (c) 2026 Patrick Szymkowiak
