#!/usr/bin/env bash
# Example: Parallel agents — 3 agents working on the same file simultaneously
#
# Demonstrates: multiple claims on different functions in the SAME file,
# parallel worktrees, sequential done (merge serialization)

set -euo pipefail

PROJECT_DIR=$(mktemp -d)
echo "=== Creating sample project in $PROJECT_DIR ==="

cd "$PROJECT_DIR"
git init -q
mkdir -p src

cat > src/auth.ts << 'EOF'
export function validateToken(token: string): boolean {
  return token.length > 0;
}

export function refreshToken(token: string): string {
  return token + "-refreshed";
}

export function revokeToken(token: string): void {
  console.log("revoked:", token);
}

export function generateToken(user: string): string {
  return btoa(user + ":" + Date.now());
}

export function parseToken(token: string): { user: string; exp: number } {
  const decoded = atob(token);
  const [user, exp] = decoded.split(":");
  return { user, exp: parseInt(exp) };
}
EOF

git add -A && git commit -q -m "initial commit"

# Initialize
grit init
echo ""
echo "=== Symbols indexed ==="
grit symbols

# 3 agents claim different functions in the SAME file
echo ""
echo "=== Agent 1: claims validateToken + parseToken ==="
grit claim -a agent-1 -i "add JWT validation" \
  "src/auth.ts::validateToken" \
  "src/auth.ts::parseToken"

echo ""
echo "=== Agent 2: claims refreshToken ==="
grit claim -a agent-2 -i "add expiry check" \
  "src/auth.ts::refreshToken"

echo ""
echo "=== Agent 3: claims generateToken ==="
grit claim -a agent-3 -i "use crypto.randomUUID" \
  "src/auth.ts::generateToken"

echo ""
echo "=== Status: 3 agents, same file, no conflicts ==="
grit status

# Agent 2 tries to claim a symbol already held by Agent 1
echo ""
echo "=== Agent 2 tries to claim parseToken (held by Agent 1) ==="
grit claim -a agent-2 -i "also want parseToken" \
  "src/auth.ts::parseToken" 2>&1 || echo "  → Blocked as expected!"

# Each agent edits in their own worktree
echo ""
echo "=== Agents work in parallel worktrees ==="
ls -la .grit/worktrees/

# Agent 1 finishes
echo ""
echo "=== Agent 1: done ==="
grit done -a agent-1

# Agent 2 finishes
echo ""
echo "=== Agent 2: done ==="
grit done -a agent-2

# Agent 3 finishes
echo ""
echo "=== Agent 3: done ==="
grit done -a agent-3

echo ""
echo "=== Final status: all locks released ==="
grit status

echo ""
echo "=== Git log: each agent's merge is a separate commit ==="
git log --oneline -10

rm -rf "$PROJECT_DIR"
