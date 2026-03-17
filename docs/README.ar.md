# grit

**Git لوكلاء الذكاء الاصطناعي — صفر تعارضات دمج، أي عدد من الوكلاء المتوازيين، نفس قاعدة الكود.**

> عندما يعمل 50 وكيلا على نفس المستودع، يتعطل git. أما Grit فلا.

Read in English: [README.md](../README.md)

---

## نتائج الاختبار (5 تكرارات × 5 جولات)

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

**Grit: صفر تعارضات عبر جميع 1,500 محاولة دمج.**

## كيف يعمل

```
                        المشكلة
  ┌─────────────────────────────────────────────────┐
  │  10 وكلاء ذكاء اصطناعي يعدلون دوال مختلفة      │
  │  في نفس الملف (auth.ts)                         │
  │                                                 │
  │  Git يرى: نفس الملف تغير على 10 فروع            │
  │  النتيجة: O(N²) تعارضات دمج                     │
  └─────────────────────────────────────────────────┘

                        الحل
  ┌─────────────────────────────────────────────────┐
  │  Grit يقفل على مستوى الدوال (AST)               │
  │  ليس على مستوى الملفات (أسطر)                   │
  │                                                 │
  │  Agent-1 يقفل: validateToken()                  │
  │  Agent-2 يقفل: refreshToken()                   │
  │  ← نفس الملف، دوال مختلفة، صفر تعارضات         │
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
  │ أو S3    │    │ worktrees│    │ file lock│
  │ lock DB  │    │ /agent-N │    │ → merge  │
  └──────────┘    └──────────┘    └──────────┘
```

## الهيكلة

```
┌─────────────────────────────────────────┐
│              مستودع git الخاص بك         │
├─────────────────────────────────────────┤
│  .grit/                                 │
│  ├── registry.db    (SQLite WAL)        │  ← فهرس الرموز + جدول الأقفال
│  ├── config.json                        │  ← إعدادات الواجهة الخلفية (محلي/S3)
│  ├── room.sock      (Unix socket)       │  ← تدفق الأحداث الفوري
│  ├── merge.lock     (RAII file lock)    │  ← تسلسل عمليات دمج git
│  └── worktrees/                         │
│      ├── agent-1/   (git worktree)      │  ← مجلد عمل معزول
│      ├── agent-2/   (git worktree)      │
│      └── agent-N/   ...                 │
├─────────────────────────────────────────┤
│  الواجهات الخلفية:                       │
│  ├── محلي: SQLite WAL (افتراضي)         │
│  ├── AWS S3 (conditional PUT)           │
│  ├── Cloudflare R2                      │
│  ├── Google Cloud Storage               │
│  ├── Azure Blob Storage                 │
│  └── MinIO (استضافة ذاتية)             │
└─────────────────────────────────────────┘
```

---

## المشكلة

تشغيل N من وكلاء الذكاء الاصطناعي بالتوازي على قاعدة الكود ينتج O(N²) تعارضات دمج. يعمل Git على مستوى **الأسطر** — عندما يعدل وكيلان دوال مختلفة في نفس الملف، يرى Git أجزاء متعارضة ويفشل الدمج.

## الحل

يقفل Grit على مستوى **AST/الدوال** باستخدام tree-sitter. كل وكيل يحجز دوال محددة قبل التعديل. الدوال المختلفة في نفس الملف لا تتعارض أبدا. يعمل الوكلاء في worktrees git معزولة ويتم تسلسل عمليات الدمج تلقائيا.

## التثبيت

```bash
cargo install --git https://github.com/rtk-ai/grit
```

## البدء السريع

```bash
cd your-project
grit init                    # تحليل AST، بناء فهرس الرموز

# الوكيل يحجز الدوال قبل التعديل
grit claim -a agent-1 -i "إضافة التحقق" \
  src/auth.ts::validateToken \
  src/auth.ts::refreshToken

# الوكيل يعمل في worktree معزولة: .grit/worktrees/agent-1/
# ... تعديل الملفات ...

# إنهاء: التزام تلقائي، إعادة تأسيس، دمج، تحرير الأقفال
grit done -a agent-1
```

## سير عمل الجلسة (تكامل GitHub)

```bash
grit session start auth-refactor        # إنشاء فرع grit/auth-refactor
# ... الوكلاء claim، يعملون، done ...
grit session pr                         # دفع الفرع + إنشاء GitHub PR
grit session end                        # تنظيف الأقفال، العودة للفرع الأساسي
```

## الأوامر

```
grit init                                    # تهيئة فهرس الرموز
grit claim -a <agent> -i <intent> <syms...>  # قفل الرموز + إنشاء worktree
grit done  -a <agent>                        # دمج + تحرير الأقفال
grit status                                  # عرض الأقفال النشطة
grit symbols [--file <pattern>]              # عرض الرموز المفهرسة
grit plan <symbols...>                       # التحقق من التوفر (dry-run)
grit release -a <agent> <symbols...>         # تحرير أقفال محددة
grit gc                                      # تنظيف الأقفال المنتهية
grit heartbeat -a <agent>                    # تحديث TTL القفل
grit watch                                   # تدفق الأحداث الفوري
grit session start|status|pr|end             # دورة حياة فرع الميزة
grit config set-s3|set-local|show            # إعدادات الواجهة الخلفية
```

## اللغات المدعومة

TypeScript, JavaScript, Rust, Python (قابل للتوسيع عبر قواعد tree-sitter)

---

## الرخصة

MIT — Copyright (c) 2026 Patrick Szymkowiak
