# grit

**AI 에이전트를 위한 Git — 머지 충돌 제로, 무제한 병렬 에이전트, 동일 코드베이스.**

> 50개의 에이전트가 같은 저장소에서 작업하면 git은 깨집니다. Grit은 깨지지 않습니다.

Read in English: [README.md](../README.md)

---

## 벤치마크 결과 (5회 반복 x 5라운드)

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

**Grit: 전체 1,500회 머지 시도에서 충돌 0건.**

## 작동 원리

```
                        문제
  ┌─────────────────────────────────────────────────┐
  │  10개의 AI 에이전트가 서로 다른 함수를 편집      │
  │  같은 파일 (auth.ts) 에서                        │
  │                                                 │
  │  Git이 보는 것: 같은 파일이 10개 브랜치에서 변경  │
  │  결과:         O(N²) 머지 충돌                   │
  └─────────────────────────────────────────────────┘

                        해결책
  ┌─────────────────────────────────────────────────┐
  │  Grit은 함수 레벨 (AST) 에서 잠금               │
  │  파일 레벨 (라인) 이 아닌                        │
  │                                                 │
  │  Agent-1 잠금: validateToken()                   │
  │  Agent-2 잠금: refreshToken()                    │
  │  → 같은 파일, 다른 함수, 충돌 제로               │
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
  │ 또는 S3  │    │ worktrees│    │ file lock│
  │ lock DB  │    │ /agent-N │    │ → merge  │
  └──────────┘    └──────────┘    └──────────┘
```

## 아키텍처

```
┌─────────────────────────────────────────┐
│              당신의 git 저장소            │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← 심볼 인덱스 + 잠금 테이블
│  ├── config.json                        │  ← 백엔드 설정 (로컬/S3)
│  ├── room.sock      (Unix 소켓)         │  ← 실시간 이벤트 스트림
│  ├── merge.lock     (RAII 파일 잠금)    │  ← git 머지 직렬화
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← 격리된 작업 디렉토리
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  백엔드:                                 │
│  ├── 로컬: SQLite WAL (기본값)           │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (셀프 호스팅)                │
└─────────────────────────────────────────┘
```

---

## 문제

코드베이스에서 N개의 AI 에이전트를 병렬로 실행하면 O(N²)의 머지 충돌이 발생합니다. Git은 **라인 레벨**에서 작동하므로, 두 에이전트가 같은 파일의 다른 함수를 편집해도 Git은 충돌하는 헝크를 감지하고 머지가 실패합니다.

## 해결책

Grit은 tree-sitter를 사용하여 **AST/함수 레벨**에서 잠금합니다. 각 에이전트는 편집 전에 특정 함수를 예약합니다. 같은 파일의 다른 함수는 절대 충돌하지 않습니다. 에이전트는 격리된 git worktree에서 작업하며, 머지는 자동으로 직렬화됩니다.

## 설치

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## 빠른 시작

```bash
cd your-project
grit init                    # AST 파싱, 심볼 인덱스 구축

# 에이전트가 편집 전에 함수를 예약
grit claim -a agent-1 -i "검증 추가" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# 에이전트는 격리된 worktree에서 작업: .grit/worktrees/agent-1/
# ... 파일 편집 ...

# 완료: 자동 커밋, 리베이스, 머지, 잠금 해제
grit done -a agent-1
```

## 세션 워크플로우 (GitHub 연동)

```bash
grit session start auth-refactor        # 브랜치 grit/auth-refactor 생성
# ... 에이전트가 claim, 작업, done ...
grit session pr                         # 브랜치 push + GitHub PR 생성
grit session end                        # 잠금 정리, 베이스 브랜치로 복귀
```

## 명령어

```
grit init                                    # 심볼 인덱스 초기화
grit claim -a <agent> -i <intent> <syms...>  # 심볼 잠금 + worktree 생성
grit done  -a <agent>                        # 머지 + 잠금 해제
grit status                                  # 활성 잠금 표시
grit symbols [--file <pattern>]              # 인덱싱된 심볼 목록
grit plan <symbols...>                       # 가용성 확인 (dry-run)
grit release -a <agent> <symbols...>         # 특정 잠금 해제
grit gc                                      # 만료된 잠금 정리
grit heartbeat -a <agent>                    # 잠금 TTL 갱신
grit watch                                   # 실시간 이벤트 스트림
grit session start|status|pr|end             # 피처 브랜치 라이프사이클
grit config set-s3|set-local|show            # 백엔드 설정
```

## 지원 언어

TypeScript, JavaScript, Rust, Python (tree-sitter 문법으로 확장 가능)

---

## 라이선스

MIT — Copyright (c) 2026 Patrick Szymkowiak
