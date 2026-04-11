#!/usr/bin/env bash
set -euo pipefail

# Start local dev infrastructure (NATS + RustFS) via Docker Compose
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

echo "Starting NATS + RustFS..."
docker compose -f "$REPO_ROOT/docker-compose.yml" up -d

echo ""
echo "Waiting for NATS..."
for i in $(seq 1 30); do
    if curl -sf http://localhost:8222/healthz > /dev/null 2>&1; then
        echo "NATS is ready (port 4222)"
        break
    fi
    sleep 1
done

echo ""
echo "Waiting for RustFS..."
for i in $(seq 1 30); do
    if curl -sf http://localhost:9100/minio/health/live > /dev/null 2>&1; then
        echo "RustFS is ready (S3: port 9100, Console: port 9101)"
        break
    fi
    sleep 1
done

echo ""
echo "Infrastructure ready:"
echo "  NATS:           nats://localhost:4222"
echo "  NATS Monitor:   http://localhost:8222"
echo "  RustFS S3:      http://localhost:9100"
echo "  RustFS Console: http://localhost:9101"
echo "  RustFS Creds:   boreadmin / boreadmin"
