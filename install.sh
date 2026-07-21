#!/usr/bin/env bash
set -euo pipefail

remove_legacy_binary() {
  local legacy_binary="${INSTALL_DIR}/pdiff"
  local response="${RAMO_REMOVE_LEGACY:-}"

  if [ ! -e "$legacy_binary" ] && [ ! -L "$legacy_binary" ]; then
    return
  fi

  if [ -z "$response" ]; then
    if [ -r /dev/tty ] && [ -w /dev/tty ]; then
      printf 'Legacy pdiff binary found at %s. Remove it? [y/N] ' "$legacy_binary" > /dev/tty
      IFS= read -r response < /dev/tty || response=""
    else
      echo "Legacy pdiff binary remains at $legacy_binary; remove it manually or rerun with RAMO_REMOVE_LEGACY=yes."
      return
    fi
  fi

  case "$response" in
    y|Y|yes|Yes|YES)
      rm -- "$legacy_binary"
      echo "Removed legacy pdiff binary from $legacy_binary"
      ;;
    *)
      echo "Kept legacy pdiff binary at $legacy_binary"
      ;;
  esac
}

main() {
  local repo="carlosarraes/ramo"
  local version="${1:-latest}"
  local os="${RAMO_INSTALL_OS:-$(uname -s | tr '[:upper:]' '[:lower:]')}"
  local arch="${RAMO_INSTALL_ARCH:-$(uname -m)}"
  local target_os target_arch target download_url

  INSTALL_DIR="${RAMO_INSTALL_DIR:-${HOME}/.local/bin}"

  case "$os" in
    linux)  target_os="unknown-linux-gnu" ;;
    darwin) target_os="apple-darwin" ;;
    *)      echo "Unsupported OS: $os"; exit 1 ;;
  esac

  case "$arch" in
    x86_64|amd64)  target_arch="x86_64" ;;
    aarch64|arm64) target_arch="aarch64" ;;
    *)             echo "Unsupported arch: $arch"; exit 1 ;;
  esac

  target="${target_arch}-${target_os}"
  if [ "$version" = "latest" ]; then
    download_url="https://github.com/${repo}/releases/latest/download/ramo-${target}.tar.gz"
  else
    download_url="https://github.com/${repo}/releases/download/${version}/ramo-${target}.tar.gz"
  fi

  echo "Installing ramo for ${target}..."
  if [ "${RAMO_INSTALL_DRY_RUN:-0}" = "1" ]; then
    echo "Download: ${download_url}"
    echo "Install: ${INSTALL_DIR}/ramo"
    return
  fi
  mkdir -p "$INSTALL_DIR"

  ramo_install_tmp="$(mktemp -d)"
  trap 'rm -rf "$ramo_install_tmp"' EXIT

  curl -fsSL "$download_url" -o "$ramo_install_tmp/ramo.tar.gz"
  tar xzf "$ramo_install_tmp/ramo.tar.gz" -C "$ramo_install_tmp"
  mv "$ramo_install_tmp/ramo" "$INSTALL_DIR/ramo"
  chmod +x "$INSTALL_DIR/ramo"

  echo "Installed ramo to $INSTALL_DIR/ramo"
  remove_legacy_binary

  if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    echo "Add to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\""
  fi
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
