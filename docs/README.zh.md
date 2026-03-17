# grit

**AI代理的Git工具 — 零合并冲突，任意数量并行代理，同一代码库。**

> 当50个代理在同一仓库工作时，git会崩溃。Grit不会。

Read in English: [README.md](../README.md)

---

## 基准测试结果 (5次迭代 x 5轮)

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

**Grit：在全部1,500次合并尝试中0冲突。**

## 工作原理

```
                        问题
  ┌─────────────────────────────────────────────────┐
  │  10个AI代理编辑不同的函数                        │
  │  在同一个文件中 (auth.ts)                        │
  │                                                 │
  │  Git看到：同一文件在10个分支上被修改              │
  │  结果：   O(N²) 合并冲突                         │
  └─────────────────────────────────────────────────┘

                        解决方案
  ┌─────────────────────────────────────────────────┐
  │  Grit在函数级别 (AST) 进行锁定                   │
  │  而非文件级别 (行)                               │
  │                                                 │
  │  Agent-1 锁定：validateToken()                   │
  │  Agent-2 锁定：refreshToken()                    │
  │  → 同一文件，不同函数，零冲突                     │
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
  │ 或 S3    │    │ worktrees│    │ file lock│
  │ lock DB  │    │ /agent-N │    │ → merge  │
  └──────────┘    └──────────┘    └──────────┘
```

## 架构

```
┌─────────────────────────────────────────┐
│              你的 git 仓库              │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← 符号索引 + 锁表
│  ├── config.json                        │  ← 后端配置 (本地/S3)
│  ├── room.sock      (Unix socket)       │  ← 实时事件流
│  ├── merge.lock     (RAII 文件锁)       │  ← 序列化 git 合并
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← 隔离的工作目录
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  后端：                                  │
│  ├── 本地：SQLite WAL (默认)             │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (自托管)                     │
└─────────────────────────────────────────┘
```

---

## 问题

在代码库上并行运行N个AI代理会产生O(N²)的合并冲突。Git在**行级别**操作——当两个代理编辑同一文件中的不同函数时，Git会看到冲突的代码块，导致合并失败。

## 解决方案

Grit使用tree-sitter在**AST/函数级别**进行锁定。每个代理在编辑前预留特定函数。同一文件中的不同函数永远不会冲突。代理在隔离的git worktree中工作，合并自动序列化。

## 安装

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## 快速开始

```bash
cd your-project
grit init                    # 解析AST，构建符号索引

# 代理在编辑前预留函数
grit claim -a agent-1 -i "添加验证" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# 代理在隔离的worktree中工作：.grit/worktrees/agent-1/
# ... 编辑文件 ...

# 完成：自动提交、变基、合并、释放锁
grit done -a agent-1
```

## 会话工作流 (GitHub集成)

```bash
grit session start auth-refactor        # 创建分支 grit/auth-refactor
# ... 代理 claim、工作、done ...
grit session pr                         # 推送分支 + 创建 GitHub PR
grit session end                        # 清理锁，返回基础分支
```

## 命令

```
grit init                                    # 初始化符号索引
grit claim -a <agent> -i <intent> <syms...>  # 锁定符号 + 创建worktree
grit done  -a <agent>                        # 合并 + 释放锁
grit status                                  # 显示活跃的锁
grit symbols [--file <pattern>]              # 列出已索引的符号
grit plan <symbols...>                       # 检查可用性 (dry-run)
grit release -a <agent> <symbols...>         # 释放特定的锁
grit gc                                      # 清理过期的锁
grit heartbeat -a <agent>                    # 刷新锁TTL
grit watch                                   # 实时事件流
grit session start|status|pr|end             # Feature分支生命周期
grit config set-s3|set-local|show            # 后端配置
```

## 支持的语言

TypeScript, JavaScript, Rust, Python (可通过tree-sitter语法扩展)

---

## 许可证

MIT — Copyright (c) 2026 Patrick Szymkowiak
