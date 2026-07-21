#!/usr/bin/env bash
set -euo pipefail

REPO="carlosarraes/pdiff"
INSTALL_DIR="${PDIFF_INSTALL_DIR:-${HOME}/.local/bin}"
VERSION="${1:-latest}"

# Detect OS and arch
OS="${PDIFF_INSTALL_OS:-$(uname -s | tr '[:upper:]' '[:lower:]')}"
ARCH="${PDIFF_INSTALL_ARCH:-$(uname -m)}"

case "$OS" in
  linux)  TARGET_OS="unknown-linux-gnu" ;;
  darwin) TARGET_OS="apple-darwin" ;;
  *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  TARGET_ARCH="x86_64" ;;
  aarch64|arm64) TARGET_ARCH="aarch64" ;;
  *)             echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

TARGET="${TARGET_ARCH}-${TARGET_OS}"

if [ "$VERSION" = "latest" ]; then
  DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/pdiff-${TARGET}.tar.gz"
else
  DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/pdiff-${TARGET}.tar.gz"
fi

echo "Installing pdiff for ${TARGET}..."
if [ "${PDIFF_INSTALL_DRY_RUN:-0}" = "1" ]; then
  echo "Download: ${DOWNLOAD_URL}"
  echo "Install: ${INSTALL_DIR}/pdiff"
  exit 0
fi
mkdir -p "$INSTALL_DIR"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$DOWNLOAD_URL" -o "$TMP/pdiff.tar.gz"
tar xzf "$TMP/pdiff.tar.gz" -C "$TMP"
mv "$TMP/pdiff" "$INSTALL_DIR/pdiff"
chmod +x "$INSTALL_DIR/pdiff"

echo "Installed pdiff to $INSTALL_DIR/pdiff"

if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
  echo "Add to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\""
fi
