#!/usr/bin/env python3
"""Generate benchmark PDF: grit vs git across agent counts."""

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.ticker as mticker
import numpy as np

# Data from sweep benchmark (5 rounds each)
agents =       [1,    2,    5,    10,   20,   50]
rounds = 5

# Raw counts from benchmark
git_fail =     [0,    5,    20,   42,   83,   175]
git_ok =       [5,    5,    5,    8,    17,   75]
grit_ok =      [5,    10,   25,   50,   100,  215]
grit_fail =    [0,    0,    0,    0,    0,    0]
git_time =     [2,    2,    5,    8,    11,   22]
grit_time =    [2,    2,    2,    3,    6,    11]
git_conflicts =[0,    35,   75,   86,   126,  175]

# Compute rates
total_runs = [a * rounds for a in agents]
git_fail_rate = [f / t * 100 for f, t in zip(git_fail, total_runs)]
grit_fail_rate = [0.0] * len(agents)
git_success_rate = [100 - r for r in git_fail_rate]
grit_success_rate = [100.0] * len(agents)

# ── Style ──
plt.rcParams.update({
    "font.family": "sans-serif",
    "font.size": 11,
    "axes.spines.top": False,
    "axes.spines.right": False,
})

GIT_COLOR = "#e74c3c"
GRIT_COLOR = "#2ecc71"
GIT_LIGHT = "#fadbd8"
GRIT_LIGHT = "#d5f5e3"

fig, axes = plt.subplots(2, 2, figsize=(14, 10))
fig.suptitle("grit vs raw git — Parallel Agent Benchmark", fontsize=18, fontweight="bold", y=0.97)
fig.text(0.5, 0.93, f"44 symbols (functions/methods) · pi-calc project · {rounds} rounds per data point",
         ha="center", fontsize=11, color="#666")

x = np.arange(len(agents))
w = 0.35

# ── 1. Merge Success Rate ──
ax = axes[0, 0]
bars1 = ax.bar(x - w/2, git_success_rate, w, label="Raw Git", color=GIT_COLOR, alpha=0.85, edgecolor="white", linewidth=0.5)
bars2 = ax.bar(x + w/2, grit_success_rate, w, label="Grit", color=GRIT_COLOR, alpha=0.85, edgecolor="white", linewidth=0.5)
ax.set_ylabel("Merge Success Rate (%)")
ax.set_xlabel("Number of Parallel Agents")
ax.set_title("Merge Success Rate", fontweight="bold", pad=10)
ax.set_xticks(x)
ax.set_xticklabels(agents)
ax.set_ylim(0, 115)
ax.legend(loc="upper right")
ax.axhline(y=100, color="#ccc", linewidth=0.5, linestyle="--")
# Add value labels
for bar in bars1:
    h = bar.get_height()
    ax.text(bar.get_x() + bar.get_width()/2., h + 1.5, f"{h:.0f}%", ha="center", va="bottom", fontsize=9, color=GIT_COLOR, fontweight="bold")
for bar in bars2:
    h = bar.get_height()
    ax.text(bar.get_x() + bar.get_width()/2., h + 1.5, f"{h:.0f}%", ha="center", va="bottom", fontsize=9, color="#27ae60", fontweight="bold")

# ── 2. Merge Failures (absolute) ──
ax = axes[0, 1]
ax.fill_between(agents, git_fail, alpha=0.2, color=GIT_COLOR)
ax.plot(agents, git_fail, "o-", color=GIT_COLOR, linewidth=2.5, markersize=8, label="Git merge failures", zorder=5)
ax.plot(agents, grit_fail, "o-", color=GRIT_COLOR, linewidth=2.5, markersize=8, label="Grit merge failures", zorder=5)
ax.fill_between(agents, grit_fail, alpha=0.2, color=GRIT_COLOR)
ax.set_ylabel(f"Failed Merges (over {rounds} rounds)")
ax.set_xlabel("Number of Parallel Agents")
ax.set_title("Merge Failures", fontweight="bold", pad=10)
ax.legend(loc="upper left")
# Annotate max
ax.annotate(f"{git_fail[-1]} failures", xy=(agents[-1], git_fail[-1]),
            xytext=(agents[-1]-12, git_fail[-1]-20),
            fontsize=10, color=GIT_COLOR, fontweight="bold",
            arrowprops=dict(arrowstyle="->", color=GIT_COLOR, lw=1.5))
ax.annotate("0 failures", xy=(agents[-1], 0),
            xytext=(agents[-1]-12, 30),
            fontsize=10, color="#27ae60", fontweight="bold",
            arrowprops=dict(arrowstyle="->", color=GRIT_COLOR, lw=1.5))

# ── 3. Conflict Files ──
ax = axes[1, 0]
ax.fill_between(agents, git_conflicts, alpha=0.15, color=GIT_COLOR)
ax.plot(agents, git_conflicts, "s-", color=GIT_COLOR, linewidth=2.5, markersize=8, label="Git conflict files", zorder=5)
ax.plot(agents, [0]*len(agents), "s-", color=GRIT_COLOR, linewidth=2.5, markersize=8, label="Grit conflict files", zorder=5)
ax.set_ylabel(f"Conflict Files (over {rounds} rounds)")
ax.set_xlabel("Number of Parallel Agents")
ax.set_title("File-Level Conflicts", fontweight="bold", pad=10)
ax.legend(loc="upper left")
for i, (a, c) in enumerate(zip(agents, git_conflicts)):
    if c > 0:
        ax.text(a, c + 4, str(c), ha="center", fontsize=9, color=GIT_COLOR, fontweight="bold")

# ── 4. Execution Time ──
ax = axes[1, 1]
bars1 = ax.bar(x - w/2, git_time, w, label="Raw Git", color=GIT_COLOR, alpha=0.85, edgecolor="white", linewidth=0.5)
bars2 = ax.bar(x + w/2, grit_time, w, label="Grit", color=GRIT_COLOR, alpha=0.85, edgecolor="white", linewidth=0.5)
ax.set_ylabel(f"Total Time (s) for {rounds} rounds")
ax.set_xlabel("Number of Parallel Agents")
ax.set_title("Execution Time", fontweight="bold", pad=10)
ax.set_xticks(x)
ax.set_xticklabels(agents)
ax.legend(loc="upper left")
for bar in bars1:
    h = bar.get_height()
    ax.text(bar.get_x() + bar.get_width()/2., h + 0.3, f"{h:.0f}s", ha="center", va="bottom", fontsize=9, color=GIT_COLOR)
for bar in bars2:
    h = bar.get_height()
    ax.text(bar.get_x() + bar.get_width()/2., h + 0.3, f"{h:.0f}s", ha="center", va="bottom", fontsize=9, color="#27ae60")

# ── Footer ──
fig.text(0.5, 0.01,
         "Adversarial scenario: all agents modify different functions in the SAME files from the SAME base commit.\n"
         "Git: branch-per-agent, sequential merge. Grit: AST-level symbol locking, parallel worktrees, serialized merge.",
         ha="center", fontsize=9, color="#888", style="italic")

plt.tight_layout(rect=[0, 0.04, 1, 0.91])

out = "/Users/patrick/dev/personnal/test-redone-git/benchmark-grit-vs-git.pdf"
fig.savefig(out, dpi=150, bbox_inches="tight")
print(f"PDF saved to: {out}")

# Also save PNG for quick preview
out_png = out.replace(".pdf", ".png")
fig.savefig(out_png, dpi=150, bbox_inches="tight")
print(f"PNG saved to: {out_png}")
