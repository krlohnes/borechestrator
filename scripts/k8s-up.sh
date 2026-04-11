#!/usr/bin/env bash
set -euo pipefail

# Deploy NATS + RustFS to Kubernetes via Helm
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
NAMESPACE="${NAMESPACE:-borechestrator}"

echo "Creating namespace $NAMESPACE..."
kubectl create namespace "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -

echo ""
echo "Adding Helm repos..."
helm repo add nats https://nats-io.github.io/k8s/helm/charts/ 2>/dev/null || true
helm repo add rustfs https://charts.rustfs.com/ 2>/dev/null || true
helm repo update

echo ""
echo "Installing NATS with JetStream..."
helm upgrade --install nats nats/nats \
    -n "$NAMESPACE" \
    -f "$REPO_ROOT/deploy/helm/values-nats.yaml" \
    --wait

echo ""
echo "Installing RustFS..."
helm upgrade --install rustfs rustfs/rustfs \
    -n "$NAMESPACE" \
    -f "$REPO_ROOT/deploy/helm/values-rustfs.yaml" \
    --wait

echo ""
echo "Infrastructure deployed to namespace $NAMESPACE:"
echo "  NATS:    nats://nats.$NAMESPACE.svc:4222"
echo "  RustFS:  http://rustfs.$NAMESPACE.svc:9000"
