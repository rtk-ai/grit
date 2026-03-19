#!/usr/bin/env bash
# Example: Session workflow — feature branch + GitHub PR
#
# Demonstrates: session start → agents work → session pr → session end
# NOTE: This example requires a GitHub remote to create PRs.
#       Run it in a real repo with `gh` CLI configured.

set -euo pipefail

echo "=== Session Workflow ==="
echo "This example shows the full session lifecycle."
echo "Run it in a real git repo with a GitHub remote."
echo ""

# 1. Start a session (creates branch grit/<name>)
echo "$ grit session start auth-refactor"
echo "  → Creates branch grit/auth-refactor"
echo "  → Records base branch for PR"
echo ""

# 2. Agents claim and work
echo "$ grit claim -a agent-1 -i 'add JWT' src/auth.ts::validateToken"
echo "$ grit claim -a agent-2 -i 'add OAuth' src/auth.ts::oauthLogin"
echo "  → Each agent gets an isolated worktree"
echo ""

# 3. Check session status
echo "$ grit session status"
echo "  → Shows: branch, base, active agents, locked symbols"
echo ""

# 4. Agents finish
echo "$ grit done -a agent-1"
echo "$ grit done -a agent-2"
echo "  → Each merge is serialized (no conflicts)"
echo ""

# 5. Create PR
echo "$ grit session pr --title 'Auth refactor: JWT + OAuth'"
echo "  → Pushes branch to origin"
echo "  → Creates GitHub PR with session summary"
echo ""

# 6. End session
echo "$ grit session end"
echo "  → Cleans up expired locks"
echo "  → Switches back to base branch"
echo ""

echo "=== Full command sequence ==="
cat << 'CMD'
grit init
grit session start auth-refactor

grit claim -a agent-1 -i "add JWT validation" src/auth.ts::validateToken
grit claim -a agent-2 -i "add OAuth flow"     src/auth.ts::oauthLogin

# ... agents edit in .grit/worktrees/agent-{1,2}/ ...

grit done -a agent-1
grit done -a agent-2

grit session pr --title "Auth refactor: JWT + OAuth"
grit session end
CMD
