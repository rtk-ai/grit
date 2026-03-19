#!/usr/bin/env bash
# Example: S3-compatible backend — distributed lock coordination
#
# Demonstrates: configuring grit to use S3/R2/MinIO for locks,
# enabling multi-machine agent coordination.

set -euo pipefail

echo "=== S3 Backend Configuration ==="
echo ""
echo "By default, grit uses SQLite (local). For distributed teams"
echo "or cloud-based agents, switch to an S3-compatible backend."
echo ""

echo "--- AWS S3 ---"
cat << 'CMD'
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...

grit init
grit config set-s3 --bucket my-grit-locks --region us-east-1
grit config show
CMD

echo ""
echo "--- Cloudflare R2 ---"
cat << 'CMD'
export AWS_ACCESS_KEY_ID=...        # R2 API token
export AWS_SECRET_ACCESS_KEY=...

grit init
grit config set-s3 \
  --bucket grit-locks \
  --endpoint https://<account-id>.r2.cloudflarestorage.com \
  --region auto
CMD

echo ""
echo "--- MinIO (self-hosted) ---"
cat << 'CMD'
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin

grit init
grit config set-s3 \
  --bucket grit-locks \
  --endpoint http://localhost:9000 \
  --region us-east-1
CMD

echo ""
echo "--- Google Cloud Storage (S3-compatible) ---"
cat << 'CMD'
export AWS_ACCESS_KEY_ID=...        # HMAC key
export AWS_SECRET_ACCESS_KEY=...

grit init
grit config set-s3 \
  --bucket grit-locks \
  --endpoint https://storage.googleapis.com \
  --region auto
CMD

echo ""
echo "--- Switch back to local ---"
cat << 'CMD'
grit config set-local
grit config show
CMD

echo ""
echo "=== How S3 locking works ==="
echo "Each lock is an S3 object:"
echo "  Key:  .grit/locks/{url_encoded_symbol_id}"
echo "  Body: JSON LockEntry (agent, intent, TTL, timestamp)"
echo ""
echo "Atomic acquisition via conditional PUT (If-None-Match: *)"
echo "  → Only one agent can create the lock object"
echo "  → Race conditions handled with retry logic"
