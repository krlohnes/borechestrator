#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

echo "Stopping NATS + RustFS..."
docker compose -f "$REPO_ROOT/docker-compose.yml" down

echo "Done."
