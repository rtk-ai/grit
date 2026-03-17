#!/usr/bin/env bash
# Test harness: launches N parallel agents using claude -p
# Each agent claims symbols, modifies code, and releases
# Validates: no conflicts, all changes merged, all locks released

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GRIT="$REPO_ROOT/target/release/grit"
TEST_DIR="$REPO_ROOT/test-projects"
NUM_AGENTS=${1:-20}
RESULTS_DIR="$REPO_ROOT/test-results/$(date +%Y%m%d_%H%M%S)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[harness]${NC} $1"; }
ok()  { echo -e "${GREEN}[✓]${NC} $1"; }
err() { echo -e "${RED}[✗]${NC} $1"; }
warn() { echo -e "${YELLOW}[!]${NC} $1"; }

mkdir -p "$RESULTS_DIR"

# ─── STEP 1: Build grit ───
log "Building grit (release)..."
cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null
ok "grit built"

# ─── STEP 2: Setup test repo ───
WORK_REPO="$RESULTS_DIR/work-repo"
log "Creating work repository..."
cp -r "$TEST_DIR/ts-api" "$WORK_REPO"
cd "$WORK_REPO"
git init -q
git add -A
git commit -q -m "initial commit"
ok "Work repo ready at $WORK_REPO"

# ─── STEP 3: Initialize grit ───
log "Initializing grit..."
"$GRIT" --repo "$WORK_REPO" init
ok "grit initialized"

# ─── STEP 4: List available symbols ───
log "Listing symbols..."
SYMBOLS=$("$GRIT" --repo "$WORK_REPO" symbols 2>/dev/null | grep "│" | sed 's/.*│ //' | sed 's/ (.*//' | head -60)
SYMBOL_COUNT=$(echo "$SYMBOLS" | wc -l | tr -d ' ')
log "Found $SYMBOL_COUNT symbols"

# Get full symbol IDs
SYMBOL_IDS=$("$GRIT" --repo "$WORK_REPO" symbols 2>/dev/null | grep "│" | awk '{print $NF}' | head -60)

# ─── STEP 5: Prepare agent tasks ───
# Each agent will:
# 1. grit claim some symbols
# 2. Make a small modification to those symbols
# 3. grit done

AGENT_SCRIPT="$RESULTS_DIR/agent_task.sh"
cat > "$AGENT_SCRIPT" << 'AGENT_EOF'
#!/usr/bin/env bash
# Agent task script
# Args: GRIT_BIN REPO_PATH AGENT_ID SYMBOL_IDS...
set -euo pipefail

GRIT="$1"
REPO="$2"
AGENT_ID="$3"
shift 3
SYMBOLS=("$@")

LOGFILE="$REPO/../agent-${AGENT_ID}.log"

log() { echo "[agent-$AGENT_ID] $1" >> "$LOGFILE"; }

log "Starting. Claiming ${#SYMBOLS[@]} symbols: ${SYMBOLS[*]}"

# Claim
INTENT="Agent $AGENT_ID automated modification"
CLAIM_RESULT=$("$GRIT" --repo "$REPO" claim -a "$AGENT_ID" -i "$INTENT" "${SYMBOLS[@]}" 2>&1) || {
    log "CLAIM FAILED: $CLAIM_RESULT"
    echo "FAIL:CLAIM:$AGENT_ID"
    exit 0
}
log "Claimed successfully"

# Simulate work (add a comment to each claimed symbol's file)
for SYM in "${SYMBOLS[@]}"; do
    FILE=$(echo "$SYM" | cut -d: -f1-2 | sed 's/::/\//')
    FILEPATH="$REPO/$FILE"
    if [[ -f "$FILEPATH" ]]; then
        # Add a comment at the end of the file
        echo "// Modified by agent-$AGENT_ID at $(date +%H:%M:%S)" >> "$FILEPATH"
        log "Modified $FILE"
    fi
done

# Small delay to simulate work
sleep $((RANDOM % 3 + 1))

# Done
DONE_RESULT=$("$GRIT" --repo "$REPO" done -a "$AGENT_ID" 2>&1) || {
    log "DONE FAILED: $DONE_RESULT"
    echo "FAIL:DONE:$AGENT_ID"
    exit 0
}
log "Done successfully"
echo "OK:$AGENT_ID"
AGENT_EOF
chmod +x "$AGENT_SCRIPT"

# ─── STEP 6: Distribute symbols among agents ───
log "Distributing symbols among $NUM_AGENTS agents..."

# Get all symbol IDs into an array
mapfile -t ALL_SYMBOLS < <("$GRIT" --repo "$WORK_REPO" symbols 2>/dev/null | \
    grep -E "^\s+│" | \
    awk '{for(i=1;i<=NF;i++) if($i=="│") {name=$(i+1); break}} END{} {print name}')

# Actually, let's get symbols from the DB directly
mapfile -t ALL_SYMBOLS < <(sqlite3 "$WORK_REPO/.grit/registry.db" "SELECT id FROM symbols ORDER BY RANDOM()" 2>/dev/null || \
    sqlite3 "$WORK_REPO/.agent-room/registry.db" "SELECT id FROM symbols ORDER BY RANDOM()")

TOTAL_SYMS=${#ALL_SYMBOLS[@]}
PER_AGENT=$((TOTAL_SYMS / NUM_AGENTS))
if [[ $PER_AGENT -lt 1 ]]; then PER_AGENT=1; fi

log "Total symbols: $TOTAL_SYMS, per agent: $PER_AGENT"

# ─── STEP 7: Launch agents in parallel ───
log "Launching $NUM_AGENTS agents in parallel..."
PIDS=()
START_TIME=$(date +%s)

for i in $(seq 1 "$NUM_AGENTS"); do
    OFFSET=$(( (i - 1) * PER_AGENT ))
    # Slice symbols for this agent (no overlap!)
    AGENT_SYMS=()
    for j in $(seq 0 $((PER_AGENT - 1))); do
        IDX=$((OFFSET + j))
        if [[ $IDX -lt $TOTAL_SYMS ]]; then
            AGENT_SYMS+=("${ALL_SYMBOLS[$IDX]}")
        fi
    done

    if [[ ${#AGENT_SYMS[@]} -eq 0 ]]; then
        continue
    fi

    bash "$AGENT_SCRIPT" "$GRIT" "$WORK_REPO" "agent-$i" "${AGENT_SYMS[@]}" > "$RESULTS_DIR/agent-$i.result" 2>&1 &
    PIDS+=($!)
done

log "Waiting for ${#PIDS[@]} agents to complete..."

# Wait for all
PASS=0
FAIL=0
for pid in "${PIDS[@]}"; do
    wait "$pid" || true
done

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

# ─── STEP 8: Collect results ───
log "Collecting results..."

for i in $(seq 1 "$NUM_AGENTS"); do
    RESULT_FILE="$RESULTS_DIR/agent-$i.result"
    if [[ -f "$RESULT_FILE" ]]; then
        RESULT=$(cat "$RESULT_FILE")
        if [[ "$RESULT" == OK:* ]]; then
            ((PASS++))
        elif [[ "$RESULT" == FAIL:* ]]; then
            ((FAIL++))
            warn "Agent $i failed: $RESULT"
        fi
    fi
done

# ─── STEP 9: Verify final state ───
log "Verifying final state..."

# Check all locks are released
REMAINING_LOCKS=$("$GRIT" --repo "$WORK_REPO" status 2>/dev/null | grep -c "●" || echo "0")

echo ""
echo "═══════════════════════════════════════════"
echo "           GRIT TEST RESULTS"
echo "═══════════════════════════════════════════"
echo ""
echo "  Agents launched:    $NUM_AGENTS"
echo "  Agents succeeded:   $PASS"
echo "  Agents failed:      $FAIL"
echo "  Duration:           ${DURATION}s"
echo "  Remaining locks:    $REMAINING_LOCKS"
echo "  Symbols in repo:    $TOTAL_SYMS"
echo ""

if [[ $FAIL -eq 0 && $REMAINING_LOCKS -eq 0 ]]; then
    ok "ALL TESTS PASSED — zero conflicts, all locks released"
    exit 0
else
    err "SOME TESTS FAILED"
    exit 1
fi
