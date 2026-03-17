#!/usr/bin/env bash
# Launch N Claude agents in parallel, each coordinating via grit
# This is the real test: AI agents using grit to avoid conflicts
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GRIT="$REPO_ROOT/target/release/grit"
NUM_AGENTS=${1:-5}  # Start with 5, scale up to 20
TEST_PROJECT=${2:-"ts-api"}
RESULTS_DIR="$REPO_ROOT/test-results/claude-$(date +%Y%m%d_%H%M%S)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[orchestrator]${NC} $1"; }
ok()  { echo -e "${GREEN}[✓]${NC} $1"; }
err() { echo -e "${RED}[✗]${NC} $1"; }

mkdir -p "$RESULTS_DIR"

# ─── Setup ───
WORK_REPO="$RESULTS_DIR/repo"
log "Setting up test repository from $TEST_PROJECT..."
cp -r "$REPO_ROOT/test-projects/$TEST_PROJECT" "$WORK_REPO"
cd "$WORK_REPO"
git init -q
git add -A
git commit -q -m "initial commit"

log "Initializing grit..."
"$GRIT" --repo "$WORK_REPO" init
ok "Grit initialized"

# Get symbols
mapfile -t ALL_SYMBOLS < <(sqlite3 "$WORK_REPO/.grit/registry.db" \
    "SELECT id FROM symbols WHERE kind IN ('function','method') ORDER BY RANDOM()" 2>/dev/null || \
    sqlite3 "$WORK_REPO/.agent-room/registry.db" \
    "SELECT id FROM symbols WHERE kind IN ('function','method') ORDER BY RANDOM()")

TOTAL_SYMS=${#ALL_SYMBOLS[@]}
PER_AGENT=$((TOTAL_SYMS / NUM_AGENTS))
[[ $PER_AGENT -lt 1 ]] && PER_AGENT=1

log "Symbols: $TOTAL_SYMS total, $PER_AGENT per agent"

# ─── Task descriptions for each agent ───
TASKS=(
    "Add input validation to the function. Check parameter types and throw descriptive errors for invalid inputs."
    "Add logging statements at the start and end of the function. Log the function name, parameters, and return value."
    "Add error handling with try-catch blocks. Catch specific error types and rethrow with more context."
    "Add JSDoc/docstring comments describing the function purpose, parameters, return value, and examples."
    "Optimize the function for performance. Add early returns, reduce unnecessary allocations."
    "Add telemetry/metrics. Track execution time, call count, and error rate."
    "Add caching where appropriate. Use a simple in-memory cache with TTL."
    "Add rate limiting checks. Throw if called too frequently."
    "Add retry logic for operations that might fail transiently."
    "Add null/undefined safety checks at the start of the function."
    "Convert callbacks to async/await if applicable. Add proper error propagation."
    "Add unit test scaffolding as comments showing expected behavior."
    "Add deprecation warnings where old patterns are used."
    "Add request/response type validation using runtime checks."
    "Refactor long functions into smaller helper functions."
    "Add timeout handling for async operations."
    "Add circuit breaker pattern for external calls."
    "Add data sanitization for user-provided inputs."
    "Add feature flag checks at the start of the function."
    "Add structured error codes instead of string error messages."
)

# ─── Launch Claude agents ───
log "Launching $NUM_AGENTS Claude agents..."
PIDS=()
START_TIME=$(date +%s)

for i in $(seq 1 "$NUM_AGENTS"); do
    AGENT_ID="claude-agent-$i"
    OFFSET=$(( (i - 1) * PER_AGENT ))

    # Get this agent's symbols
    SYMS=()
    for j in $(seq 0 $((PER_AGENT - 1))); do
        IDX=$((OFFSET + j))
        if [[ $IDX -lt $TOTAL_SYMS ]]; then
            SYMS+=("${ALL_SYMBOLS[$IDX]}")
        fi
    done

    [[ ${#SYMS[@]} -eq 0 ]] && continue

    TASK_IDX=$(( (i - 1) % ${#TASKS[@]} ))
    TASK="${TASKS[$TASK_IDX]}"
    SYM_LIST=$(printf '"%s" ' "${SYMS[@]}")

    AGENT_PROMPT="You are agent '$AGENT_ID' working on the repository at '$WORK_REPO'.
You MUST use the grit CLI at '$GRIT' to coordinate with other agents.

STEP 1: Claim your symbols
Run: $GRIT --repo $WORK_REPO claim -a $AGENT_ID -i \"$TASK\" $SYM_LIST

If any symbol is BLOCKED, skip it and work only on the granted ones.

STEP 2: For each GRANTED symbol, make this modification:
$TASK

The symbols are in the format 'file::function_name'.
Read the file, find the function, and make the modification.
Keep changes minimal and focused on the assigned symbols only.

STEP 3: When done, release your locks:
Run: $GRIT --repo $WORK_REPO done -a $AGENT_ID

IMPORTANT:
- ONLY modify functions that you have successfully claimed
- Do NOT modify any other code
- Keep changes small and focused
- If claim fails for a symbol, SKIP it
- Always run 'done' at the end even if some work failed"

    log "Launching agent $i with ${#SYMS[@]} symbols: ${SYMS[*]:0:3}..."

    claude -p "$AGENT_PROMPT" > "$RESULTS_DIR/agent-$i.log" 2>&1 &
    PIDS+=($!)

    # Stagger launches slightly to reduce thundering herd
    sleep 0.5
done

log "All $NUM_AGENTS agents launched. Waiting..."

# ─── Wait and collect ───
for pid in "${PIDS[@]}"; do
    wait "$pid" 2>/dev/null || true
done

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

# ─── Verify ───
log "Verifying results..."

LOCKS_REMAINING=$("$GRIT" --repo "$WORK_REPO" status 2>/dev/null | grep -c "●" || echo "0")

# Check git status for conflicts
cd "$WORK_REPO"
GIT_STATUS=$(git status --porcelain 2>/dev/null | head -20)
CONFLICT_COUNT=$(echo "$GIT_STATUS" | grep -c "^UU" || echo "0")

# Count successful agents (those that logged "Done successfully" or similar)
PASS=0
FAIL=0
for i in $(seq 1 "$NUM_AGENTS"); do
    LOGFILE="$RESULTS_DIR/agent-$i.log"
    if [[ -f "$LOGFILE" ]]; then
        if grep -qi "released\|done\|✓" "$LOGFILE" 2>/dev/null; then
            ((PASS++))
        else
            ((FAIL++))
        fi
    fi
done

echo ""
echo "═══════════════════════════════════════════════════"
echo "        GRIT + CLAUDE AGENTS TEST RESULTS"
echo "═══════════════════════════════════════════════════"
echo ""
echo "  Test project:       $TEST_PROJECT"
echo "  Agents launched:    $NUM_AGENTS"
echo "  Agents succeeded:   $PASS"
echo "  Agents failed:      $FAIL"
echo "  Duration:           ${DURATION}s"
echo "  Remaining locks:    $LOCKS_REMAINING"
echo "  Git conflicts:      $CONFLICT_COUNT"
echo "  Total symbols:      $TOTAL_SYMS"
echo ""
echo "  Results dir:        $RESULTS_DIR"
echo ""

if [[ $CONFLICT_COUNT -eq 0 ]]; then
    ok "ZERO GIT CONFLICTS with $NUM_AGENTS parallel agents!"
else
    err "$CONFLICT_COUNT conflicts detected"
fi

if [[ $LOCKS_REMAINING -eq 0 ]]; then
    ok "All locks properly released"
else
    warn "$LOCKS_REMAINING locks still held (agents may have crashed)"
fi

echo ""
echo "Agent logs:"
for i in $(seq 1 "$NUM_AGENTS"); do
    SIZE=$(wc -c < "$RESULTS_DIR/agent-$i.log" 2>/dev/null || echo "0")
    echo "  agent-$i: $(du -h "$RESULTS_DIR/agent-$i.log" 2>/dev/null | cut -f1) — $(tail -1 "$RESULTS_DIR/agent-$i.log" 2>/dev/null | head -c 80)"
done
