#!/usr/bin/env bash
set -euo pipefail

NAMESPACE="${NAMESPACE:-borechestrator}"

echo "Uninstalling from namespace $NAMESPACE..."
helm uninstall nats -n "$NAMESPACE" 2>/dev/null || true
helm uninstall rustfs -n "$NAMESPACE" 2>/dev/null || true

echo "Done. PVCs are retained. To fully clean up:"
echo "  kubectl delete namespace $NAMESPACE"
