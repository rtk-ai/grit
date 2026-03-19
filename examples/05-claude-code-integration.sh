#!/usr/bin/env bash
# Example: Using grit with Claude Code (or any AI coding tool)
#
# Demonstrates: how an AI agent can use grit to coordinate
# with other agents on the same codebase.

set -euo pipefail

echo "=== Grit + Claude Code Integration ==="
echo ""
echo "Add this to your CLAUDE.md or agent system prompt:"
echo ""

cat << 'PROMPT'
## Multi-Agent Coordination (grit)

This repo uses grit for parallel agent coordination.
Before editing functions, claim them to prevent conflicts.

### Before editing
```bash
# See what symbols exist
grit symbols --file src/auth.ts

# Claim the functions you need
grit claim -a $AGENT_ID -i "your intent" \
  "src/auth.ts::validateToken" \
  "src/auth.ts::refreshToken"
```

### After editing
```bash
# Merge your changes and release locks
grit done -a $AGENT_ID
```

### Check for conflicts
```bash
# See who holds what
grit status

# Plan before claiming
grit plan -a $AGENT_ID -i "add caching to auth"
```

### If blocked
```bash
# Wait and retry with heartbeat
grit heartbeat -a $AGENT_ID  # refresh your TTL

# Or check what's available
grit symbols --file src/auth.ts
# → shows which functions are free vs locked
```
PROMPT

echo ""
echo "=== Multi-agent orchestration script ==="
echo ""

cat << 'SCRIPT'
#!/usr/bin/env bash
# Launch 3 Claude Code agents in parallel with grit coordination

REPO_DIR=$(pwd)
grit init
grit session start feature-sprint

# Agent 1: backend auth
claude -p "You are agent-1. Run: grit claim -a agent-1 -i 'auth' src/auth.ts::validateToken
Edit the function in .grit/worktrees/agent-1/src/auth.ts
Then run: grit done -a agent-1" &

# Agent 2: backend API
claude -p "You are agent-2. Run: grit claim -a agent-2 -i 'api' src/api.ts::handleRequest
Edit the function in .grit/worktrees/agent-2/src/api.ts
Then run: grit done -a agent-2" &

# Agent 3: frontend
claude -p "You are agent-3. Run: grit claim -a agent-3 -i 'ui' src/App.tsx::render
Edit the function in .grit/worktrees/agent-3/src/App.tsx
Then run: grit done -a agent-3" &

# Wait for all agents
wait

# Create PR with all changes
grit session pr --title "Feature sprint: auth + API + UI"
grit session end
SCRIPT
