#!/usr/bin/env zsh
set -uo pipefail

GRIT=/Users/patrick/dev/personnal/test-redone-git/target/release/grit
PROJ=/Users/patrick/dev/personnal/test-redone-git/test-projects/pi-calc
RESULTS_DIR=/tmp/grit-sweep-$(date +%Y%m%d_%H%M%S)
CSV="$RESULTS_DIR/results.csv"
mkdir -p "$RESULTS_DIR"

NUM_ROUNDS=5
AGENT_COUNTS=(1 2 5 10 20 50)

# Helper: modify a function
modify_function() {
    local FILE="$1" FUNC="$2" TAG="$3" DIR="$4"
    local FILEPATH="$DIR/$FILE"
    [[ -f "$FILEPATH" ]] || return 0
    local LINE=$(grep -n "fn ${FUNC}\b\|function ${FUNC}\b\|const ${FUNC}\b" "$FILEPATH" 2>/dev/null | head -1 | cut -d: -f1)
    if [[ -n "$LINE" ]] && [[ "$LINE" -gt 0 ]]; then
        local INSERT=$((LINE + 1))
        if [[ "$FILE" == *.rs ]]; then
            sed -i '' "${INSERT}i\\
    // === ${TAG} ===\\
    // Modified by ${TAG}\\
    // ts: $(date +%s%N)\\
    // ok\\
    // === end ===
" "$FILEPATH" 2>/dev/null
        elif [[ "$FILE" == *.ts ]] || [[ "$FILE" == *.tsx ]]; then
            sed -i '' "${INSERT}i\\
  // === ${TAG} ===\\
  // Modified by ${TAG}\\
  // ts: $(date +%s%N)\\
  // ok\\
  // === end ===
" "$FILEPATH" 2>/dev/null
        fi
    fi
}

add_file_header() {
    local FILE="$1" TAG="$2" DIR="$3"
    local FILEPATH="$DIR/$FILE"
    [[ -f "$FILEPATH" ]] || return 0
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
rm -rf "$TMP_IDX"

echo "agents,git_ok,git_fail,git_conflict_files,git_fail_rate,git_time,grit_ok,grit_conflicts,grit_time" > "$CSV"

echo "=================================================================="
echo "  SWEEP BENCHMARK: grit vs raw git"
echo "  Agent counts: ${AGENT_COUNTS[*]}"
echo "  $NUM_ROUNDS rounds each, $SYM_COUNT symbols"
echo "=================================================================="
echo ""

for NUM_AGENTS in "${AGENT_COUNTS[@]}"; do
    echo "--- Testing with $NUM_AGENTS agents ---"

    # ── GIT ──
    GIT_OK=0; GIT_FAIL=0; GIT_CONFLICTS=0; GIT_TIME=0

    for ROUND in $(seq 1 $NUM_ROUNDS); do
        WORK="$RESULTS_DIR/git-n${NUM_AGENTS}-r$ROUND"
        rm -rf "$WORK"
        cp -r "$PROJ" "$WORK"
        cd "$WORK"
        git init -q && git add -A && git commit -q -m "init"

        SYMS=("${(@f)$(sqlite3 "$SYM_DB" "SELECT id FROM symbols WHERE kind IN ('function','method') ORDER BY RANDOM()")}")
        TOTAL=${#SYMS[@]}

        START_T=$SECONDS
        MAIN_BRANCH=$(git branch --show-current)

        for i in $(seq 1 $NUM_AGENTS); do
            git checkout -q "$MAIN_BRANCH"
            git checkout -q -b "agent-$i"

            K=$i
            MODIFIED_FILES=()
            while [[ $K -le $TOTAL ]]; do
                SYM="${SYMS[$K]}"
                modify_function "${SYM%%::*}" "${SYM##*::}" "a$i-r$ROUND" "$WORK"
                FILE="${SYM%%::*}"
                if [[ ! " ${MODIFIED_FILES[*]:-} " =~ " $FILE " ]]; then
                    MODIFIED_FILES+=("$FILE")
                fi
                K=$((K + NUM_AGENTS))
            done

            for FILE in "${MODIFIED_FILES[@]:-}"; do
                [[ -n "$FILE" ]] && add_file_header "$FILE" "A$i-R$ROUND" "$WORK"
            done

            git add -A 2>/dev/null
            git commit -q -m "agent-$i r$ROUND" 2>/dev/null
        done

        git checkout -q "$MAIN_BRANCH"
        for i in $(seq 1 $NUM_AGENTS); do
            git merge --no-ff "agent-$i" -m "merge agent-$i" >/dev/null 2>&1
            if [[ $? -eq 0 ]]; then
                GIT_OK=$((GIT_OK + 1))
            else
                GIT_FAIL=$((GIT_FAIL + 1))
                CONF=$(git diff --name-only --diff-filter=U 2>/dev/null | wc -l | tr -d ' ')
                GIT_CONFLICTS=$((GIT_CONFLICTS + CONF))
                git merge --abort 2>/dev/null
            fi
        done

        GIT_TIME=$((GIT_TIME + SECONDS - START_T))
        rm -rf "$WORK"
    done

    TOTAL_RUNS=$((NUM_ROUNDS * NUM_AGENTS))
    GIT_RATE=$(echo "scale=1; $GIT_FAIL * 100 / $TOTAL_RUNS" | bc)
    printf "  GIT:  %3d runs  ok=%3d  fail=%3d  (%.1f%%)  %ds\n" "$TOTAL_RUNS" "$GIT_OK" "$GIT_FAIL" "$GIT_RATE" "$GIT_TIME"

    # ── GRIT ──
    GRIT_OK=0; GRIT_CONFLICTS=0; GRIT_TIME=0

    for ROUND in $(seq 1 $NUM_ROUNDS); do
        WORK="$RESULTS_DIR/grit-n${NUM_AGENTS}-r$ROUND"
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
                "$GRIT" --repo "$WORK" claim -a "n${NUM_AGENTS}r${ROUND}a${i}" -i "task$i" "${AGENT_SYMS[@]}" >/dev/null 2>&1 || exit 1
                WT="$WORK/.grit/worktrees/n${NUM_AGENTS}r${ROUND}a${i}"
                if [[ -d "$WT" ]]; then
                    for SYM in "${AGENT_SYMS[@]}"; do
                        modify_function "${SYM%%::*}" "${SYM##*::}" "a$i-r$ROUND" "$WT"
                    done
                fi
                "$GRIT" --repo "$WORK" done -a "n${NUM_AGENTS}r${ROUND}a${i}" >/dev/null 2>&1
            ) &
        done

        wait
        GRIT_TIME=$((GRIT_TIME + SECONDS - START_T))

        cd "$WORK"
        CONF=$(git status --porcelain 2>/dev/null | grep -c "^UU" || true)
        MERGES=$(git log --oneline 2>/dev/null | grep -c "grit: merge" || true)
        GRIT_CONFLICTS=$((GRIT_CONFLICTS + CONF))
        GRIT_OK=$((GRIT_OK + MERGES))

        rm -rf "$WORK"
    done

    printf "  GRIT: %3d runs  ok=%3d  conflicts=%d  %ds\n" "$TOTAL_RUNS" "$GRIT_OK" "$GRIT_CONFLICTS" "$GRIT_TIME"
    echo ""

    echo "$NUM_AGENTS,$GIT_OK,$GIT_FAIL,$GIT_CONFLICTS,$GIT_RATE,$GIT_TIME,$GRIT_OK,$GRIT_CONFLICTS,$GRIT_TIME" >> "$CSV"
done

echo "CSV saved to: $CSV"
cat "$CSV"
