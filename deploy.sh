#!/usr/bin/env bash
set -euo pipefail

# One-command release deploy for Boomergames -> GCP VM (behind nginx + PostgreSQL).
# Usage:
#   ./deploy.sh               # rebuild WASM + web + boomer-server, upload, restart, verify
#   ./deploy.sh --skip-build  # upload & restart using already-built artifacts

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

VM_NAME="${BOOMER_VM_NAME:-bleiglanzinfo}"
VM_ZONE="${BOOMER_VM_ZONE:-us-west1-b}"
SITE_URL="${BOOMER_SITE_URL:-https://games.bleiglanz.info}"
SKIP_BUILD=false

for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=true ;;
    -h|--help)
      echo "Usage: ./deploy.sh [--skip-build]"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      exit 1
      ;;
  esac
done

GCLOUD="$(command -v gcloud || true)"
if [[ -z "$GCLOUD" && -x "$HOME/google-cloud-sdk/bin/gcloud" ]]; then
  GCLOUD="$HOME/google-cloud-sdk/bin/gcloud"
fi
if [[ -z "$GCLOUD" ]]; then
  echo "Error: gcloud CLI not found in PATH or \$HOME/google-cloud-sdk/bin/gcloud" >&2
  exit 1
fi

if [[ "$SKIP_BUILD" == "false" ]]; then
  echo "==> Building WASM + frontend (make build)..."
  make build
  echo "==> Building release boomer-server..."
  cargo build --release -p boomer-server
fi

if [[ ! -x "$ROOT_DIR/target/release/boomer-server" || ! -f "$ROOT_DIR/web/dist/index.html" ]]; then
  echo "Error: build artifacts missing. Run without --skip-build." >&2
  exit 1
fi

TARBALL="$(mktemp /tmp/boomergames-release-XXXXXX.tar.gz)"
trap 'rm -f "$TARBALL"' EXIT

echo "==> Packaging release bundle..."
tar -czf "$TARBALL" -C "$ROOT_DIR/target/release" boomer-server -C "$ROOT_DIR/web" dist
ls -lh "$TARBALL"

echo "==> Uploading to ${VM_NAME} (${VM_ZONE})..."
"$GCLOUD" compute scp "$TARBALL" "${VM_NAME}:/tmp/boomergames-release.tar.gz" \
  --zone="$VM_ZONE" --quiet

echo "==> Installing and restarting boomergames.service..."
"$GCLOUD" compute ssh "$VM_NAME" --zone="$VM_ZONE" --quiet --ssh-flag="-o BatchMode=yes" --command='
  set -euo pipefail
  STAGE=$(mktemp -d /tmp/boomer-stage-XXXXXX)
  trap "rm -rf \"$STAGE\" /tmp/boomergames-release.tar.gz" EXIT

  tar -xzf /tmp/boomergames-release.tar.gz -C "$STAGE"
  sudo install -m 755 -o boomergames -g boomergames "$STAGE/boomer-server" /opt/boomergames/boomer-server.new
  sudo rm -rf /opt/boomergames/dist.new
  sudo mv "$STAGE/dist" /opt/boomergames/dist.new
  sudo chown -R boomergames:boomergames /opt/boomergames/dist.new

  sudo systemctl stop boomergames.service
  sudo mv -f /opt/boomergames/boomer-server.new /opt/boomergames/boomer-server
  sudo rm -rf /opt/boomergames/dist.old
  if [ -d /opt/boomergames/dist ]; then
    sudo mv /opt/boomergames/dist /opt/boomergames/dist.old
  fi
  sudo mv /opt/boomergames/dist.new /opt/boomergames/dist
  sudo rm -rf /opt/boomergames/dist.old
  sudo systemctl start boomergames.service

  sleep 2
  sudo systemctl is-active --quiet boomergames.service
'

echo "==> Verifying ${SITE_URL}/status..."
curl -sSf "${SITE_URL}/status"
echo ""
echo "==> Deploy complete!"
