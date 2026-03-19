#!/usr/bin/env bash
# Example: Basic grit workflow — single agent
#
# Demonstrates: init → claim → work → done

set -euo pipefail

PROJECT_DIR=$(mktemp -d)
echo "=== Creating sample project in $PROJECT_DIR ==="

# Create a sample git repo with some code
cd "$PROJECT_DIR"
git init -q
mkdir -p src

cat > src/math.ts << 'EOF'
export function add(a: number, b: number): number {
  return a + b;
}

export function subtract(a: number, b: number): number {
  return a - b;
}

export function multiply(a: number, b: number): number {
  return a * b;
}

export function divide(a: number, b: number): number {
  if (b === 0) throw new Error("division by zero");
  return a / b;
}
EOF

git add -A && git commit -q -m "initial commit"

# 1. Initialize grit
echo ""
echo "=== Step 1: grit init ==="
grit init

# 2. See what symbols are available
echo ""
echo "=== Step 2: grit symbols ==="
grit symbols

# 3. Agent claims functions to work on
echo ""
echo "=== Step 3: Agent claims 'add' and 'subtract' ==="
grit claim -a agent-1 -i "add input validation" \
  "src/math.ts::add" \
  "src/math.ts::subtract"

# 4. Check lock status
echo ""
echo "=== Step 4: grit status ==="
grit status

# 5. Agent works in its isolated worktree
echo ""
echo "=== Step 5: Agent edits files in worktree ==="
WORKTREE="$PROJECT_DIR/.grit/worktrees/agent-1"
cat > "$WORKTREE/src/math.ts" << 'EOF'
export function add(a: number, b: number): number {
  if (typeof a !== "number" || typeof b !== "number") throw new Error("invalid input");
  return a + b;
}

export function subtract(a: number, b: number): number {
  if (typeof a !== "number" || typeof b !== "number") throw new Error("invalid input");
  return a - b;
}

export function multiply(a: number, b: number): number {
  return a * b;
}

export function divide(a: number, b: number): number {
  if (b === 0) throw new Error("division by zero");
  return a / b;
}
EOF

# 6. Done: auto-commit, rebase, merge, release locks
echo ""
echo "=== Step 6: grit done ==="
grit done -a agent-1

# 7. Verify clean state
echo ""
echo "=== Step 7: Final status (should be empty) ==="
grit status

echo ""
echo "=== Done! Changes merged back to main branch ==="
git log --oneline -5

# Cleanup
rm -rf "$PROJECT_DIR"
