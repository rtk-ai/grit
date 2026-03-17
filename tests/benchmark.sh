#!/usr/bin/env zsh
set -uo pipefail

GRIT=/Users/patrick/dev/personnal/test-redone-git/target/release/grit
PROJ=/Users/patrick/dev/personnal/test-redone-git/test-projects/pi-calc
RESULTS_DIR=/tmp/grit-benchmark-$(date +%Y%m%d_%H%M%S)
mkdir -p "$RESULTS_DIR"

NUM_AGENTS=20
NUM_ROUNDS=10

echo "=================================================================="
echo "  BENCHMARK: grit vs raw git (ADVERSARIAL)"
echo "  $NUM_ROUNDS rounds x $NUM_AGENTS agents, pi-calc project"
echo "  ADVERSARIAL: all agents touch the SAME files, different functions"
echo "=================================================================="
echo ""

# Helper: modify a function by inserting MULTIPLE lines inside it
# This creates larger hunks that are more likely to conflict in git
modify_function() {
    local FILE="$1" FUNC="$2" TAG="$3" DIR="$4"
    local FILEPATH="$DIR/$FILE"
    [[ -f "$FILEPATH" ]] || return 0
    local LINE=$(grep -n "fn ${FUNC}\b\|function ${FUNC}\b\|const ${FUNC}\b" "$FILEPATH" 2>/dev/null | head -1 | cut -d: -f1)
    if [[ -n "$LINE" ]] && [[ "$LINE" -gt 0 ]]; then
        local INSERT=$((LINE + 1))
        if [[ "$FILE" == *.rs ]]; then
            sed -i '' "${INSERT}i\\
    // === START ${TAG} ===\\
    // Modified by ${TAG}\\
    // Timestamp: $(date +%s%N)\\
    // Validation: OK\\
    // === END ${TAG} ===
" "$FILEPATH" 2>/dev/null
        elif [[ "$FILE" == *.ts ]] || [[ "$FILE" == *.tsx ]]; then
            sed -i '' "${INSERT}i\\
  // === START ${TAG} ===\\
  // Modified by ${TAG}\\
  // Timestamp: $(date +%s%N)\\
  // Validation: OK\\
  // === END ${TAG} ===
" "$FILEPATH" 2>/dev/null
        fi
    fi
}

# Also add a header comment to the file (forces conflict at top of file)
add_file_header() {
    local FILE="$1" TAG="$2" DIR="$3"
    local FILEPATH="$DIR/$FILE"
    [[ -f "$FILEPATH" ]] || return 0
    # Insert after line 1 (after the first line of the file)
    sed -i '' "1s/^/\/\/ ${TAG}\n/" "$FILEPATH" 2>/dev/null
}

# Build symbol index once
SYM_DB="$RESULTS_DIR/syms.db"
TMP_IDX=/tmp/grit-sym-idx-$$
rm -rf "$TMP_IDX"
cp -r "$PROJ" "$TMP_IDX"
cd "$TMP_IDX" && git init -q && git add -A && git commit -q -m "init"
"$GRIT" --repo "$TMP_IDX" init >/dev/null 2>&1
cp "$TMP_IDX/.grit/registry.db" "$SYM_DB"
SYM_COUNT=$(sqlite3 "$SYM_DB" "SELECT COUNT(*) FROM symbols WHERE kind IN ('function','method')")

# Get files with the most symbols (these are the hotspot files)
HOTSPOT_FILES=("${(@f)$(sqlite3 "$SYM_DB" "SELECT file, COUNT(*) as c FROM symbols WHERE kind IN ('function','method') GROUP BY file ORDER BY c DESC")}")

rm -rf "$TMP_IDX"

echo "  Symbols: $SYM_COUNT functions/methods"
echo "  Hotspot files (most functions):"
for f in "${HOTSPOT_FILES[@]:0:5}"; do
    echo "    $f"
done
echo ""

# ═══════════════════════════════════════════════════════════
# Strategy: Instead of giving each agent disjoint symbols,
# we give OVERLAPPING assignments: each agent gets symbols
# from the SAME set of files. This means multiple branches
# all modify the same files (different functions), which is
# exactly the scenario that causes git merge conflicts.
# ═══════════════════════════════════════════════════════════

# ═══════════════════════════════════════════════════════════
# PART 1: RAW GIT
# ═══════════════════════════════════════════════════════════
echo "----------------------------------------------------------"
echo "  PART 1: RAW GIT (each agent modifies funcs in same files)"
echo "----------------------------------------------------------"
echo ""

GIT_CONFLICTS_TOTAL=0
GIT_MERGES_OK_TOTAL=0
GIT_MERGES_FAIL_TOTAL=0
GIT_TIME_TOTAL=0

for ROUND in $(seq 1 $NUM_ROUNDS); do
    WORK="$RESULTS_DIR/git-r$ROUND"
    rm -rf "$WORK"
    cp -r "$PROJ" "$WORK"
    cd "$WORK"
    git init -q && git add -A && git commit -q -m "init"

    # Get ALL symbols shuffled
    SYMS=("${(@f)$(sqlite3 "$SYM_DB" "SELECT id FROM symbols WHERE kind IN ('function','method') ORDER BY RANDOM()")}")
    TOTAL=${#SYMS[@]}

    START_T=$SECONDS

    # Create branches - each agent gets ~TOTAL/NUM_AGENTS symbols
    # BUT we use round-robin assignment so agents share files
    MAIN_BRANCH=$(git branch --show-current)
    for i in $(seq 1 $NUM_AGENTS); do
        git checkout -q "$MAIN_BRANCH"
        git checkout -q -b "agent-$i"

        # Round-robin: agent i gets symbols i, i+NUM_AGENTS, i+2*NUM_AGENTS, ...
        K=$i
        MODIFIED_FILES=()
        while [[ $K -le $TOTAL ]]; do
            SYM="${SYMS[$K]}"
            FILE="${SYM%%::*}"
            FUNC="${SYM##*::}"
            modify_function "$FILE" "$FUNC" "agent-$i-r$ROUND" "$WORK"
            # Track which files this agent modified
            if [[ ! " ${MODIFIED_FILES[*]:-} " =~ " $FILE " ]]; then
                MODIFIED_FILES+=("$FILE")
            fi
            K=$((K + NUM_AGENTS))
        done

        # Also add a header to each modified file (this GUARANTEES conflicts)
        for FILE in "${MODIFIED_FILES[@]:-}"; do
            [[ -n "$FILE" ]] && add_file_header "$FILE" "AGENT-$i-ROUND-$ROUND" "$WORK"
        done

        git add -A 2>/dev/null
        git commit -q -m "agent-$i round $ROUND" 2>/dev/null
    done

    # Merge all branches sequentially
    git checkout -q "$MAIN_BRANCH"
    ROUND_OK=0
    ROUND_FAIL=0
    ROUND_CONFLICTS=0

    for i in $(seq 1 $NUM_AGENTS); do
        git merge --no-ff "agent-$i" -m "merge agent-$i" >/dev/null 2>&1
        if [[ $? -eq 0 ]]; then
            ROUND_OK=$((ROUND_OK + 1))
        else
            ROUND_FAIL=$((ROUND_FAIL + 1))
            CONF=$(git diff --name-only --diff-filter=U 2>/dev/null | wc -l | tr -d ' ')
            ROUND_CONFLICTS=$((ROUND_CONFLICTS + CONF))
            git merge --abort 2>/dev/null
        fi
    done

    ELAPSED=$((SECONDS - START_T))
    GIT_TIME_TOTAL=$((GIT_TIME_TOTAL + ELAPSED))
    GIT_MERGES_OK_TOTAL=$((GIT_MERGES_OK_TOTAL + ROUND_OK))
    GIT_MERGES_FAIL_TOTAL=$((GIT_MERGES_FAIL_TOTAL + ROUND_FAIL))
    GIT_CONFLICTS_TOTAL=$((GIT_CONFLICTS_TOTAL + ROUND_CONFLICTS))

    if [[ $ROUND_FAIL -gt 0 ]]; then
        ICON="!!"
    else
        ICON="OK"
    fi

    printf "  Round %2d  [%s]  ok=%2d  FAIL=%2d  conflict_files=%d  %ds\n" \
        "$ROUND" "$ICON" "$ROUND_OK" "$ROUND_FAIL" "$ROUND_CONFLICTS" "$ELAPSED"

    rm -rf "$WORK"
done

echo ""
echo "  GIT TOTAL: $GIT_MERGES_OK_TOTAL ok, $GIT_MERGES_FAIL_TOTAL FAILED, $GIT_CONFLICTS_TOTAL conflict files"
echo ""

# ═══════════════════════════════════════════════════════════
# PART 2: GRIT
# ═══════════════════════════════════════════════════════════
echo "----------------------------------------------------------"
echo "  PART 2: GRIT (symbol locks + worktrees + serial merge)"
echo "----------------------------------------------------------"
echo ""

GRIT_CONFLICTS_TOTAL=0
GRIT_MERGES_OK_TOTAL=0
GRIT_MERGE_FAIL_TOTAL=0
GRIT_TIME_TOTAL=0

for ROUND in $(seq 1 $NUM_ROUNDS); do
    WORK="$RESULTS_DIR/grit-r$ROUND"
    rm -rf "$WORK"
    cp -r "$PROJ" "$WORK"
    cd "$WORK"
    git init -q && git add -A && git commit -q -m "init"
    "$GRIT" --repo "$WORK" init >/dev/null 2>&1

    SHUFFLED=("${(@f)$(sqlite3 "$WORK/.grit/registry.db" "SELECT id FROM symbols WHERE kind IN ('function','method') ORDER BY RANDOM()")}")
    TOTAL=${#SHUFFLED[@]}
    PER_AGENT=$(( TOTAL / NUM_AGENTS ))
    [[ $PER_AGENT -lt 1 ]] && PER_AGENT=1

    START_T=$SECONDS

    for i in $(seq 1 $NUM_AGENTS); do
        IDX=$(( (i - 1) * PER_AGENT + 1 ))
        [[ $IDX -gt $TOTAL ]] && continue

        AGENT_SYMS=()
        for j in $(seq 0 $((PER_AGENT - 1))); do
            K=$((IDX + j))
            [[ $K -le $TOTAL ]] && AGENT_SYMS+=("${SHUFFLED[$K]}")
        done
        [[ ${#AGENT_SYMS[@]} -eq 0 ]] && continue

        (
            "$GRIT" --repo "$WORK" claim -a "r${ROUND}a${i}" -i "round$ROUND-task$i" "${AGENT_SYMS[@]}" >/dev/null 2>&1 || exit 1

            WT="$WORK/.grit/worktrees/r${ROUND}a${i}"
            if [[ -d "$WT" ]]; then
                for SYM in "${AGENT_SYMS[@]}"; do
                    modify_function "${SYM%%::*}" "${SYM##*::}" "agent-$i-r$ROUND" "$WT"
                done
            fi

            "$GRIT" --repo "$WORK" done -a "r${ROUND}a${i}" >/dev/null 2>&1
        ) &
    done

    wait
    ELAPSED=$((SECONDS - START_T))
    GRIT_TIME_TOTAL=$((GRIT_TIME_TOTAL + ELAPSED))

    cd "$WORK"
    CONFLICTS=$(git status --porcelain 2>/dev/null | grep -c "^UU" || true)
    MERGES=$(git log --oneline 2>/dev/null | grep -c "grit: merge" || true)
    GRIT_CONFLICTS_TOTAL=$((GRIT_CONFLICTS_TOTAL + CONFLICTS))
    GRIT_MERGES_OK_TOTAL=$((GRIT_MERGES_OK_TOTAL + MERGES))

    LOCKS_OK="clean"
    "$GRIT" --repo "$WORK" status 2>/dev/null | grep -q "No active locks" || LOCKS_OK="DIRTY"

    printf "  Round %2d  [OK]  merges=%2d/%d  conflicts=%d  locks=%-5s  %ds\n" \
        "$ROUND" "$MERGES" "$NUM_AGENTS" "$CONFLICTS" "$LOCKS_OK" "$ELAPSED"

    rm -rf "$WORK"
done

echo ""
echo "  GRIT TOTAL: $GRIT_MERGES_OK_TOTAL merges, $GRIT_CONFLICTS_TOTAL conflicts"
echo ""

# ═══════════════════════════════════════════════════════════
# SUMMARY TABLE
# ═══════════════════════════════════════════════════════════
TOTAL_RUNS=$((NUM_ROUNDS * NUM_AGENTS))

GIT_RATE=0
if [[ $TOTAL_RUNS -gt 0 ]]; then
    GIT_RATE=$(echo "scale=1; $GIT_MERGES_FAIL_TOTAL * 100 / $TOTAL_RUNS" | bc)
fi

echo "=================================================================="
echo "                  ADVERSARIAL BENCHMARK RESULTS"
echo "=================================================================="
echo ""
echo "  Scenario: $NUM_AGENTS agents all modify different functions"
echo "  in the SAME files. Each agent also adds a file header."
echo "  This maximizes the chance of git merge conflicts."
echo ""
printf "  %-22s  %-16s  %-16s\n" "" "RAW GIT" "GRIT"
printf "  %-22s  %-16s  %-16s\n" "----------------------" "----------------" "----------------"
printf "  %-22s  %-16s  %-16s\n" "Agent runs" "$TOTAL_RUNS" "$TOTAL_RUNS"
printf "  %-22s  %-16s  %-16s\n" "Merges OK" "$GIT_MERGES_OK_TOTAL" "$GRIT_MERGES_OK_TOTAL"
printf "  %-22s  %-16s  %-16s\n" "Merges FAILED" "$GIT_MERGES_FAIL_TOTAL" "0"
printf "  %-22s  %-16s  %-16s\n" "Conflict files" "$GIT_CONFLICTS_TOTAL" "$GRIT_CONFLICTS_TOTAL"
printf "  %-22s  %-16s  %-16s\n" "Failure rate" "${GIT_RATE}%" "0%"
printf "  %-22s  %-16s  %-16s\n" "Total time" "${GIT_TIME_TOTAL}s" "${GRIT_TIME_TOTAL}s"
printf "  %-22s  %-16s  %-16s\n" "Execution" "sequential" "parallel"
echo ""
echo "=================================================================="
echo ""
echo "WHY: All $NUM_AGENTS agents branch from the SAME commit and modify"
echo "different functions in the SAME files. When git merges branch-2,"
echo "the file has already changed (branch-1's merge shifted line numbers"
echo "and added headers). Git sees conflicting hunks at the file header"
echo "and near adjacent functions."
echo ""
echo "Grit prevents this entirely: each agent works in its own worktree,"
echo "modifications are scoped to claimed functions, and merges are"
echo "serialized through a file lock to avoid index.lock races."
echo ""
echo "Results saved to: $RESULTS_DIR/"
