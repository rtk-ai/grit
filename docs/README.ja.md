# grit

**AIエージェントのためのGit — マージコンフリクトゼロ、並列エージェント数無制限、同一コードベース。**

> 50のエージェントが同じリポジトリで作業すると、gitは壊れます。Gritは壊れません。

Read in English: [README.md](../README.md)

---

## ベンチマーク結果 (5イテレーション x 5ラウンド)

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

**Grit: 全1,500回のマージ試行でコンフリクト0件。**

## 仕組み

```
                        問題
  ┌─────────────────────────────────────────────────┐
  │  10個のAIエージェントが異なる関数を編集          │
  │  同じファイル (auth.ts) 内で                     │
  │                                                 │
  │  Gitの認識: 同じファイルが10ブランチで変更       │
  │  結果:      O(N²) マージコンフリクト             │
  └─────────────────────────────────────────────────┘

                        解決策
  ┌─────────────────────────────────────────────────┐
  │  Gritは関数レベル (AST) でロック                 │
  │  ファイルレベル (行) ではない                    │
  │                                                 │
  │  Agent-1がロック: validateToken()                │
  │  Agent-2がロック: refreshToken()                 │
  │  → 同じファイル、異なる関数、コンフリクトゼロ    │
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

## アーキテクチャ

```
┌─────────────────────────────────────────┐
│              あなたのgitリポジトリ        │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← シンボルインデックス + ロックテーブル
│  ├── config.json                        │  ← バックエンド設定 (ローカル/S3)
│  ├── room.sock      (Unixソケット)      │  ← リアルタイムイベントストリーム
│  ├── merge.lock     (RAIIファイルロック) │  ← gitマージをシリアライズ
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← 分離された作業ディレクトリ
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  バックエンド:                           │
│  ├── ローカル: SQLite WAL (デフォルト)   │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (セルフホスト)               │
└─────────────────────────────────────────┘
```

---

## 問題

N個のAIエージェントをコードベース上で並列実行すると、O(N²)のマージコンフリクトが発生します。Gitは**行レベル**で動作するため、2つのエージェントが同じファイル内の異なる関数を編集しても、Gitは競合するハンクを検出しマージが失敗します。

## 解決策

Gritはtree-sitterを使用して**AST/関数レベル**でロックします。各エージェントは編集前に特定の関数を予約します。同じファイル内の異なる関数は決してコンフリクトしません。エージェントは分離されたgit worktreeで作業し、マージは自動的にシリアライズされます。

## インストール

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## クイックスタート

```bash
cd your-project
grit init                    # ASTを解析、シンボルインデックスを構築

# エージェントが編集前に関数を予約
grit claim -a agent-1 -i "バリデーション追加" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# エージェントは分離されたworktreeで作業: .grit/worktrees/agent-1/
# ... ファイルを編集 ...

# 完了: 自動コミット、リベース、マージ、ロック解放
grit done -a agent-1
```

## セッションワークフロー (GitHub連携)

```bash
grit session start auth-refactor        # ブランチ grit/auth-refactor を作成
# ... エージェントがclaim、作業、done ...
grit session pr                         # ブランチをpush + GitHub PRを作成
grit session end                        # ロックをクリーンアップ、ベースブランチに戻る
```

## コマンド

```
grit init                                    # シンボルインデックスを初期化
grit claim -a <agent> -i <intent> <syms...>  # シンボルをロック + worktree作成
grit done  -a <agent>                        # マージ + ロック解放
grit status                                  # アクティブなロックを表示
grit symbols [--file <pattern>]              # インデックス済みシンボルを一覧
grit plan <symbols...>                       # 可用性を確認 (dry-run)
grit release -a <agent> <symbols...>         # 特定のロックを解放
grit gc                                      # 期限切れロックをクリーンアップ
grit heartbeat -a <agent>                    # ロックTTLを更新
grit watch                                   # リアルタイムイベントストリーム
grit session start|status|pr|end             # フィーチャーブランチライフサイクル
grit config set-s3|set-local|show            # バックエンド設定
```

## 対応言語

TypeScript, JavaScript, Rust, Python (tree-sitterグラマーで拡張可能)

---

## ライセンス

MIT — Copyright (c) 2026 Patrick Szymkowiak
