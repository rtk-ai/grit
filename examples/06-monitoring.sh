#!/usr/bin/env bash
# Example: Monitoring and maintenance
#
# Demonstrates: status, watch, gc, heartbeat

set -euo pipefail

echo "=== Monitoring & Maintenance ==="
echo ""

echo "--- Real-time event stream ---"
cat << 'CMD'
# Terminal 1: start watching
grit watch
# Output:
#   [CLAIMED]  agent=agent-1 symbols=[src/auth.ts::login]
#   [CLAIMED]  agent=agent-2 symbols=[src/api.ts::handler]
#   [DONE]     agent=agent-1 symbols=[src/auth.ts::login]
CMD

echo ""
echo "--- Lock status ---"
cat << 'CMD'
grit status
# Output:
#   * agent-1 -- add validation
#     | src/auth.ts::validateToken (2026-03-19T14:30:00Z) [ttl=600s]
#     | src/auth.ts::parseToken (2026-03-19T14:30:00Z) [ttl=600s]
#   * agent-2 -- fix OAuth flow
#     | src/auth.ts::oauthLogin (2026-03-19T14:31:00Z) [ttl=600s]
#
#   3/44 symbols locked
CMD

echo ""
echo "--- Heartbeat (keep locks alive) ---"
cat << 'CMD'
# Refresh TTL for long-running agents
grit heartbeat -a agent-1 --ttl 1800    # extend to 30 minutes

# Useful in a loop for very long tasks:
while agent_is_running; do
  grit heartbeat -a agent-1
  sleep 300   # every 5 minutes
done
CMD

echo ""
echo "--- Garbage collection ---"
cat << 'CMD'
# Clean up expired locks (agent crashed, TTL exceeded)
grit gc
# Output: Cleaned up 2 expired locks.

# Run periodically via cron:
# */5 * * * * cd /path/to/repo && grit gc
CMD

echo ""
echo "--- Symbol inspection ---"
cat << 'CMD'
# List all symbols
grit symbols

# Filter by file
grit symbols --file auth

# Plan: check availability + get claim command
grit plan -a my-agent -i "refactor auth middleware"
# Output:
#   Planning for: refactor auth middleware
#   Relevant symbols:
#     > src/auth.ts::validateToken [function] FREE
#     > src/auth.ts::refreshToken [function] LOCKED (agent-2)
#     > src/auth.ts::revokeToken [function] FREE
#
#   Claim with:
#     grit claim -a my-agent -i "refactor auth middleware" \
#       "src/auth.ts::validateToken" "src/auth.ts::revokeToken"
CMD
