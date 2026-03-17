#!/usr/bin/env python3
"""Generate benchmark PDF: grit vs git with architecture explanation."""

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
import matplotlib.ticker as mticker
import numpy as np

# Averaged data from 5 parallel iterations (5 rounds each)
# Format per iteration: agents, git_ok, git_fail, git_conflicts, ...
# ITER 1: 1,5,0,0 | 2,5,5,39 | 5,5,20,88 | 10,7,43,99 | 20,16,84,136 | 50,75,175,175
# ITER 2: 1,5,0,0 | 2,5,5,38 | 5,5,20,85 | 10,8,42,88 | 20,15,85,129 | 50,75,175,175
# ITER 3: 1,5,0,0 | 2,5,5,37 | 5,5,20,77 | 10,7,43,85 | 20,17,83,122 | 50,75,175,175
# ITER 4: 1,5,0,0 | 2,5,5,37 | 5,5,20,63 | 10,6,44,88 | 20,17,83,126 | 50,75,175,175
# ITER 5: 1,5,0,0 | 2,5,5,38 | 5,5,20,85 | 10,6,44,89 | 20,18,82,136 | 50,75,175,175

agents =       [1,    2,    5,    10,   20,   50]
rounds = 5
n_iterations = 5

# Per-iteration data [iter][agent_idx]
_git_fail_all = [
    [0, 5, 20, 43, 84, 175],
    [0, 5, 20, 42, 85, 175],
    [0, 5, 20, 43, 83, 175],
    [0, 5, 20, 44, 83, 175],
    [0, 5, 20, 44, 82, 175],
]
_git_ok_all = [
    [5, 5, 5, 7, 16, 75],
    [5, 5, 5, 8, 15, 75],
    [5, 5, 5, 7, 17, 75],
    [5, 5, 5, 6, 17, 75],
    [5, 5, 5, 6, 18, 75],
]
_git_conflicts_all = [
    [0, 39, 88, 99, 136, 175],
    [0, 38, 85, 88, 129, 175],
    [0, 37, 77, 85, 122, 175],
    [0, 37, 63, 88, 126, 175],
    [0, 38, 85, 89, 136, 175],
]

# Average across iterations
git_fail =     [np.mean([it[i] for it in _git_fail_all]) for i in range(len(agents))]
git_ok =       [np.mean([it[i] for it in _git_ok_all]) for i in range(len(agents))]
git_conflicts = [np.mean([it[i] for it in _git_conflicts_all]) for i in range(len(agents))]
grit_fail =    [0,    0,    0,    0,    0,    0]

# Std dev for error bars
git_fail_std =     [np.std([it[i] for it in _git_fail_all]) for i in range(len(agents))]
git_conflicts_std = [np.std([it[i] for it in _git_conflicts_all]) for i in range(len(agents))]

# Min/max for range display
git_fail_min = [min(it[i] for it in _git_fail_all) for i in range(len(agents))]
git_fail_max = [max(it[i] for it in _git_fail_all) for i in range(len(agents))]
git_conflicts_min = [min(it[i] for it in _git_conflicts_all) for i in range(len(agents))]
git_conflicts_max = [max(it[i] for it in _git_conflicts_all) for i in range(len(agents))]

git_time =     [2,    4,    5,    7,    10,   20]
grit_time =    [2,    2,    3,    5,    7,    11]

total_runs = [a * rounds for a in agents]
git_fail_rate = [f / t * 100 for f, t in zip(git_fail, total_runs)]
git_success_rate = [100 - r for r in git_fail_rate]
grit_success_rate = [100.0] * len(agents)

# ── Style ──
plt.rcParams.update({
    "font.family": "sans-serif",
    "font.size": 10,
    "axes.spines.top": False,
    "axes.spines.right": False,
})

GIT_COLOR = "#e74c3c"
GRIT_COLOR = "#2ecc71"
GRIT_DARK = "#27ae60"
GIT_DARK = "#c0392b"
BLUE = "#3498db"
ORANGE = "#e67e22"
PURPLE = "#9b59b6"
GRAY = "#95a5a6"
DARK = "#2c3e50"

fig = plt.figure(figsize=(16, 22))

# ═══════════════════════════════════════════════════════════
# PAGE TITLE
# ═══════════════════════════════════════════════════════════
fig.text(0.5, 0.97, "grit", fontsize=42, fontweight="bold", ha="center", color=DARK, fontfamily="monospace")
fig.text(0.5, 0.955, "AST-level coordination layer for parallel AI agents on top of git",
         fontsize=13, ha="center", color=GRAY)
fig.text(0.5, 0.943, "github.com/rtk-ai/grit", fontsize=10, ha="center", color=BLUE, style="italic")

# ═══════════════════════════════════════════════════════════
# SECTION 1: THE PROBLEM (top area)
# ═══════════════════════════════════════════════════════════
ax_problem = fig.add_axes([0.05, 0.855, 0.9, 0.075])
ax_problem.set_xlim(0, 10)
ax_problem.set_ylim(0, 1)
ax_problem.axis("off")

ax_problem.text(0.0, 0.9, "THE PROBLEM", fontsize=14, fontweight="bold", color=GIT_DARK, va="top")
ax_problem.text(0.0, 0.5,
    "10 AI agents work in parallel on a codebase. Each creates a git branch, modifies different functions\n"
    "in the same files, then merges back. Git operates at the LINE level — when branches diverge from the\n"
    "same commit and touch the same file, merge conflicts explode. Even if agents edit different functions.",
    fontsize=10, va="top", color=DARK, linespacing=1.5)

# ═══════════════════════════════════════════════════════════
# SECTION 2: GIT FLOW vs GRIT FLOW (diagrams)
# ═══════════════════════════════════════════════════════════

# ── Git flow diagram ──
ax_git_flow = fig.add_axes([0.03, 0.695, 0.45, 0.155])
ax_git_flow.set_xlim(0, 10)
ax_git_flow.set_ylim(0, 6)
ax_git_flow.axis("off")
ax_git_flow.set_title("Raw Git — branch per agent, sequential merge", fontsize=11, fontweight="bold", color=GIT_DARK, pad=8)

# Main branch line
ax_git_flow.plot([0.5, 9.5], [3, 3], color=DARK, linewidth=3, zorder=5)
ax_git_flow.text(0.2, 3, "main", fontsize=9, fontweight="bold", va="center", ha="right", color=DARK)

# Agent branches
n_show = 5
colors_agents = [BLUE, ORANGE, PURPLE, "#1abc9c", "#e91e63"]
y_positions = [5.2, 4.4, 3, 1.6, 0.8]
for i in range(n_show):
    x_start = 1.0
    x_end = 3.0 + i * 1.2
    y = y_positions[i]
    # Branch out
    ax_git_flow.annotate("", xy=(x_start + 0.5, y), xytext=(x_start, 3),
                         arrowprops=dict(arrowstyle="-", color=colors_agents[i], lw=1.5, linestyle="--"))
    # Work line
    ax_git_flow.plot([x_start + 0.5, x_end], [y, y], color=colors_agents[i], linewidth=2)
    ax_git_flow.text(x_start + 0.3, y + 0.25, f"agent-{i+1}", fontsize=7, color=colors_agents[i])
    # Merge attempt
    if i < 2:
        ax_git_flow.annotate("", xy=(x_end + 0.3, 3), xytext=(x_end, y),
                             arrowprops=dict(arrowstyle="->", color=GRIT_COLOR, lw=1.5))
        ax_git_flow.text(x_end + 0.4, 3.3, "OK", fontsize=7, color=GRIT_DARK, fontweight="bold")
    else:
        ax_git_flow.annotate("", xy=(x_end + 0.3, 3), xytext=(x_end, y),
                             arrowprops=dict(arrowstyle="->", color=GIT_COLOR, lw=1.5))
        ax_git_flow.text(x_end + 0.1, 2.3 if y < 3 else 3.4, "CONFLICT!", fontsize=7, color=GIT_DARK, fontweight="bold")

ax_git_flow.text(5, 0.1, "Sequential merge: agent-3+ conflicts because file changed by agent-1,2",
                 fontsize=8, ha="center", color=GRAY, style="italic")

# ── Grit flow diagram ──
ax_grit_flow = fig.add_axes([0.52, 0.695, 0.45, 0.155])
ax_grit_flow.set_xlim(0, 10)
ax_grit_flow.set_ylim(0, 6)
ax_grit_flow.axis("off")
ax_grit_flow.set_title("Grit — symbol locks + worktrees + serialized merge", fontsize=11, fontweight="bold", color=GRIT_DARK, pad=8)

# Session branch
ax_grit_flow.plot([0.5, 9.5], [3, 3], color=DARK, linewidth=3, zorder=5)
ax_grit_flow.text(0.0, 3, "session", fontsize=9, fontweight="bold", va="center", ha="right", color=DARK)

# Lock acquisition phase
ax_grit_flow.add_patch(mpatches.FancyBboxPatch((1.0, 4.5), 2.5, 0.8, boxstyle="round,pad=0.1",
    facecolor="#eaf2f8", edgecolor=BLUE, linewidth=1.5))
ax_grit_flow.text(2.25, 4.9, "1. CLAIM", fontsize=9, fontweight="bold", ha="center", color=BLUE)
ax_grit_flow.text(2.25, 4.6, "lock symbols", fontsize=7, ha="center", color=BLUE)

# Parallel work phase
ax_grit_flow.add_patch(mpatches.FancyBboxPatch((4.0, 4.5), 2.5, 0.8, boxstyle="round,pad=0.1",
    facecolor="#eafaf1", edgecolor=GRIT_COLOR, linewidth=1.5))
ax_grit_flow.text(5.25, 4.9, "2. WORK", fontsize=9, fontweight="bold", ha="center", color=GRIT_DARK)
ax_grit_flow.text(5.25, 4.6, "parallel worktrees", fontsize=7, ha="center", color=GRIT_DARK)

# Merge phase
ax_grit_flow.add_patch(mpatches.FancyBboxPatch((7.0, 4.5), 2.5, 0.8, boxstyle="round,pad=0.1",
    facecolor="#fef9e7", edgecolor=ORANGE, linewidth=1.5))
ax_grit_flow.text(8.25, 4.9, "3. DONE", fontsize=9, fontweight="bold", ha="center", color=ORANGE)
ax_grit_flow.text(8.25, 4.6, "rebase + merge", fontsize=7, ha="center", color=ORANGE)

# Arrows between phases
ax_grit_flow.annotate("", xy=(4.0, 4.9), xytext=(3.5, 4.9),
                      arrowprops=dict(arrowstyle="->", color=DARK, lw=1.5))
ax_grit_flow.annotate("", xy=(7.0, 4.9), xytext=(6.5, 4.9),
                      arrowprops=dict(arrowstyle="->", color=DARK, lw=1.5))

# Agent worktrees (parallel)
for i in range(n_show):
    x_start = 4.2
    x_end = 6.3
    y = 0.5 + i * 0.5
    ax_grit_flow.plot([x_start, x_end], [y, y], color=colors_agents[i], linewidth=2)
    ax_grit_flow.text(x_start - 0.2, y, f"wt-{i+1}", fontsize=6, color=colors_agents[i], ha="right", va="center")
    # Merge arrow
    ax_grit_flow.annotate("", xy=(8.0, 3), xytext=(x_end, y),
                         arrowprops=dict(arrowstyle="->", color=GRIT_COLOR, lw=1, alpha=0.5))

ax_grit_flow.text(8.5, 2.4, "ALL OK", fontsize=9, color=GRIT_DARK, fontweight="bold", ha="center")
ax_grit_flow.text(5, 0.0, "Parallel work, serialized merge via file lock: zero conflicts",
                 fontsize=8, ha="center", color=GRAY, style="italic")

# ═══════════════════════════════════════════════════════════
# SECTION 3: HOW GRIT WORKS (step by step for 10 agents)
# ═══════════════════════════════════════════════════════════
ax_howto = fig.add_axes([0.05, 0.54, 0.9, 0.145])
ax_howto.set_xlim(0, 20)
ax_howto.set_ylim(0, 5)
ax_howto.axis("off")

ax_howto.text(0, 4.8, "HOW IT WORKS — 10 agents on auth.ts (8 functions)", fontsize=14, fontweight="bold", color=DARK, va="top")

steps = [
    ("1", "grit init", "Parse AST with tree-sitter\nIndex 8 functions into SQLite", BLUE, 0),
    ("2", "grit session start", "Create branch grit/improve-auth\nfrom current main", BLUE, 4),
    ("3", "grit claim -a agent-N", "Lock functions atomically\n+ create git worktree per agent", ORANGE, 8),
    ("4", "agents work in parallel", "Each in .grit/worktrees/agent-N/\nOnly edit claimed functions", GRIT_DARK, 12),
    ("5", "grit done -a agent-N", "Rebase on session branch\nMerge + release locks", PURPLE, 16),
]

for num, title, desc, color, x in steps:
    ax_howto.add_patch(mpatches.FancyBboxPatch((x, 0.5), 3.5, 3.5, boxstyle="round,pad=0.15",
        facecolor="white", edgecolor=color, linewidth=2))
    ax_howto.add_patch(mpatches.Circle((x + 0.4, 3.6), 0.3, facecolor=color, edgecolor="white", linewidth=2))
    ax_howto.text(x + 0.4, 3.6, num, fontsize=10, fontweight="bold", ha="center", va="center", color="white")
    ax_howto.text(x + 1.8, 3.3, title, fontsize=8, fontweight="bold", ha="center", va="top", color=color)
    ax_howto.text(x + 1.8, 2.3, desc, fontsize=7, ha="center", va="top", color=DARK, linespacing=1.4)

    if x < 16:
        ax_howto.annotate("", xy=(x + 3.7, 2.2), xytext=(x + 3.5, 2.2),
                         arrowprops=dict(arrowstyle="->", color=GRAY, lw=1.5))

# Final step
ax_howto.text(10, 0.15, "Then:  grit session pr  →  push branch + create GitHub PR  →  review  →  merge to main",
              fontsize=9, ha="center", color=DARK, fontweight="bold",
              bbox=dict(boxstyle="round,pad=0.3", facecolor="#eaf2f8", edgecolor=BLUE, linewidth=1.5))

# ═══════════════════════════════════════════════════════════
# SECTION 4: BENCHMARK CHARTS
# ═══════════════════════════════════════════════════════════
fig.text(0.5, 0.52, "BENCHMARK RESULTS", fontsize=16, fontweight="bold", ha="center", color=DARK)
fig.text(0.5, 0.508, "Adversarial scenario: all agents modify different functions in the SAME files — averaged over 5 iterations × 5 rounds",
         fontsize=9, ha="center", color=GRAY)

x = np.arange(len(agents))
w = 0.35

# ── Chart 1: Success Rate ──
ax1 = fig.add_axes([0.06, 0.34, 0.42, 0.155])
bars1 = ax1.bar(x - w/2, git_success_rate, w, label="Raw Git", color=GIT_COLOR, alpha=0.85, edgecolor="white")
bars2 = ax1.bar(x + w/2, grit_success_rate, w, label="Grit", color=GRIT_COLOR, alpha=0.85, edgecolor="white")
ax1.set_ylabel("Success Rate (%)")
ax1.set_xlabel("Parallel Agents")
ax1.set_title("Merge Success Rate", fontweight="bold", fontsize=11, pad=8)
ax1.set_xticks(x)
ax1.set_xticklabels(agents)
ax1.set_ylim(0, 118)
ax1.legend(loc="upper right", fontsize=8)
ax1.axhline(y=100, color="#ccc", linewidth=0.5, linestyle="--")
for bar in bars1:
    h = bar.get_height()
    ax1.text(bar.get_x() + bar.get_width()/2., h + 1.5, f"{h:.0f}%", ha="center", fontsize=7, color=GIT_DARK, fontweight="bold")
for bar in bars2:
    h = bar.get_height()
    ax1.text(bar.get_x() + bar.get_width()/2., h + 1.5, f"{h:.0f}%", ha="center", fontsize=7, color=GRIT_DARK, fontweight="bold")

# ── Chart 2: Failures (with error bars from 5 iterations) ──
ax2 = fig.add_axes([0.55, 0.34, 0.42, 0.155])
ax2.fill_between(agents, git_fail_min, git_fail_max, alpha=0.12, color=GIT_COLOR, label="Git range (5 iter)")
ax2.plot(agents, git_fail, "o-", color=GIT_COLOR, linewidth=2.5, markersize=7, label="Git avg failures", zorder=5)
ax2.plot(agents, grit_fail, "o-", color=GRIT_COLOR, linewidth=2.5, markersize=7, label="Grit failures", zorder=5)
ax2.set_ylabel(f"Failed Merges (avg {n_iterations} iter)")
ax2.set_xlabel("Parallel Agents")
ax2.set_title("Merge Failures", fontweight="bold", fontsize=11, pad=8)
ax2.legend(loc="upper left", fontsize=7)
ax2.annotate(f"{git_fail[-1]:.0f}", xy=(agents[-1], git_fail[-1]),
            xytext=(agents[-1]-10, git_fail[-1]-15), fontsize=10, color=GIT_DARK, fontweight="bold",
            arrowprops=dict(arrowstyle="->", color=GIT_COLOR, lw=1.5))
ax2.annotate("0 (all 5 iter)", xy=(agents[-1], 0), xytext=(agents[-1]-10, 25),
            fontsize=9, color=GRIT_DARK, fontweight="bold",
            arrowprops=dict(arrowstyle="->", color=GRIT_COLOR, lw=1.5))

# ── Chart 3: Conflict Files (with range from 5 iterations) ──
ax3 = fig.add_axes([0.06, 0.14, 0.42, 0.155])
ax3.fill_between(agents, git_conflicts_min, git_conflicts_max, alpha=0.12, color=GIT_COLOR, label="Git range (5 iter)")
ax3.plot(agents, git_conflicts, "s-", color=GIT_COLOR, linewidth=2.5, markersize=7, label="Git avg", zorder=5)
ax3.plot(agents, [0]*len(agents), "s-", color=GRIT_COLOR, linewidth=2.5, markersize=7, label="Grit", zorder=5)
ax3.set_ylabel(f"Conflict Files (avg {n_iterations} iter)")
ax3.set_xlabel("Parallel Agents")
ax3.set_title("File-Level Conflicts", fontweight="bold", fontsize=11, pad=8)
ax3.legend(loc="upper left", fontsize=7)
for a, c, cmin, cmax in zip(agents, git_conflicts, git_conflicts_min, git_conflicts_max):
    if c > 0:
        ax3.text(a, c + 6, f"{c:.0f}\n[{cmin}-{cmax}]", ha="center", fontsize=6, color=GIT_DARK, fontweight="bold")

# ── Chart 4: Execution Time ──
ax4 = fig.add_axes([0.55, 0.14, 0.42, 0.155])
bars1 = ax4.bar(x - w/2, git_time, w, label="Raw Git (sequential)", color=GIT_COLOR, alpha=0.85, edgecolor="white")
bars2 = ax4.bar(x + w/2, grit_time, w, label="Grit (parallel)", color=GRIT_COLOR, alpha=0.85, edgecolor="white")
ax4.set_ylabel(f"Time (s) for {rounds} rounds")
ax4.set_xlabel("Parallel Agents")
ax4.set_title("Execution Time", fontweight="bold", fontsize=11, pad=8)
ax4.set_xticks(x)
ax4.set_xticklabels(agents)
ax4.legend(loc="upper left", fontsize=8)
for bar in bars1:
    h = bar.get_height()
    ax4.text(bar.get_x() + bar.get_width()/2., h + 0.3, f"{h:.0f}s", ha="center", fontsize=7, color=GIT_DARK)
for bar in bars2:
    h = bar.get_height()
    ax4.text(bar.get_x() + bar.get_width()/2., h + 0.3, f"{h:.0f}s", ha="center", fontsize=7, color=GRIT_DARK)

# ═══════════════════════════════════════════════════════════
# SECTION 5: BACKEND ARCHITECTURE
# ═══════════════════════════════════════════════════════════
ax_arch = fig.add_axes([0.05, 0.01, 0.9, 0.11])
ax_arch.set_xlim(0, 20)
ax_arch.set_ylim(0, 4)
ax_arch.axis("off")

ax_arch.text(0, 3.8, "BACKENDS", fontsize=12, fontweight="bold", color=DARK, va="top")

backends = [
    ("Local\n(default)", "SQLite WAL\n1 machine", BLUE, 0),
    ("AWS S3", "Conditional PUT\nmulti-machine", ORANGE, 4),
    ("Cloudflare R2", "S3-compatible\nzero egress", "#f39c12", 8),
    ("GCS / Azure", "S3-compatible\ngateway", PURPLE, 12),
    ("MinIO", "Self-hosted\non-prem", GRIT_DARK, 16),
]

for title, desc, color, bx in backends:
    ax_arch.add_patch(mpatches.FancyBboxPatch((bx, 0.3), 3.5, 3.0, boxstyle="round,pad=0.15",
        facecolor="white", edgecolor=color, linewidth=2))
    ax_arch.text(bx + 1.75, 2.8, title, fontsize=9, fontweight="bold", ha="center", va="top", color=color)
    ax_arch.text(bx + 1.75, 1.5, desc, fontsize=8, ha="center", va="center", color=DARK, linespacing=1.3)
    if bx < 16:
        ax_arch.plot([bx + 3.5, bx + 4.0], [1.8, 1.8], color=GRAY, linewidth=1, linestyle=":")

out = "/Users/patrick/dev/personnal/test-redone-git/benchmark-grit-vs-git.pdf"
fig.savefig(out, dpi=150, bbox_inches="tight")
print(f"PDF saved to: {out}")

out_png = out.replace(".pdf", ".png")
fig.savefig(out_png, dpi=150, bbox_inches="tight")
print(f"PNG saved to: {out_png}")
