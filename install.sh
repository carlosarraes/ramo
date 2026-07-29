#!/usr/bin/env bash
set -euo pipefail

remove_legacy_binary() {
  local legacy_binary="${INSTALL_DIR}/pdiff"
  local response="${RAMO_REMOVE_LEGACY:-}"

  if [ ! -e "$legacy_binary" ] && [ ! -L "$legacy_binary" ]; then
    return
  fi

  if [ -z "$response" ]; then
    if { exec 3<>/dev/tty; } 2>/dev/null; then
      printf 'Legacy pdiff binary found at %s. Remove it? [y/N] ' "$legacy_binary" >&3
      IFS= read -r response <&3 || response=""
      exec 3>&-
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

resolve_version() {
  local requested="$1"
  local repo="$2"
  local response resolved

  if [ "$requested" != "latest" ] || [ "${RAMO_INSTALL_DRY_RUN:-0}" = "1" ]; then
    printf '%s\n' "$requested"
    return
  fi

  response="$(curl -fsSL "https://api.github.com/repos/${repo}/releases/latest")" || {
    echo "Unable to resolve the latest Ramo release from GitHub." >&2
    return 1
  }
  resolved="$(
    printf '%s\n' "$response" |
      sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
      head -n 1
  )"
  if [ -z "$resolved" ]; then
    echo "Unable to resolve the latest Ramo release from GitHub." >&2
    return 1
  fi
  printf '%s\n' "$resolved"
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
  if [ "$version" = "latest" ] && [ "${RAMO_INSTALL_DRY_RUN:-0}" != "1" ]; then
    echo "Resolving latest Ramo release..."
  fi
  version="$(resolve_version "$version" "$repo")"
  if [ "$version" = "latest" ]; then
    download_url="https://github.com/${repo}/releases/latest/download/ramo-${target}.tar.gz"
  else
    download_url="https://github.com/${repo}/releases/download/${version}/ramo-${target}.tar.gz"
  fi

  echo "Downloading ramo ${version} for ${target}..."
  if [ "${RAMO_INSTALL_DRY_RUN:-0}" = "1" ]; then
    echo "Download: ${download_url}"
    echo "Install: ${INSTALL_DIR}/ramo"
    echo "Install: ${INSTALL_DIR}/ramo-server"
    return
  fi
  mkdir -p "$INSTALL_DIR"

  ramo_install_tmp="$(mktemp -d)"
  trap 'rm -rf "$ramo_install_tmp"' EXIT

  curl -fsSL "$download_url" -o "$ramo_install_tmp/ramo.tar.gz"
  tar xzf "$ramo_install_tmp/ramo.tar.gz" -C "$ramo_install_tmp"
  if [ ! -f "$ramo_install_tmp/ramo" ] || [ ! -f "$ramo_install_tmp/ramo-server" ]; then
    echo "The Ramo release archive is missing ramo or ramo-server." >&2
    exit 1
  fi
  chmod +x "$ramo_install_tmp/ramo" "$ramo_install_tmp/ramo-server"
  mv "$ramo_install_tmp/ramo" "$INSTALL_DIR/ramo"
  mv "$ramo_install_tmp/ramo-server" "$INSTALL_DIR/ramo-server"

  echo "Installed ramo and ramo-server to $INSTALL_DIR"
  echo "Run 'ramo server setup' to enable private mobile AI analysis."
  remove_legacy_binary

  if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    echo "Add to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\""
  fi
}

if [ "${BASH_SOURCE[0]:-$0}" = "$0" ]; then
  main "$@"
fi
