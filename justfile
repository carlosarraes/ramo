build:
    cargo build --release
    mkdir -p ~/.local/bin
    cp target/release/ramo ~/.local/bin/
    @echo "Installed ramo to ~/.local/bin/"

install-pi:
    ~/.local/bin/ramo install pi
    @echo "Pi extension installed"

install: build install-pi

# Run the same Rust checks expected by CI.
check:
    just --fmt --check
    cargo fmt --all -- --check
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    cargo test --locked --workspace --all-targets --all-features
    bash tests/release_recipe.sh

# Cut an exact release, then let the tag-triggered workflow publish artifacts.
# Usage: just release 0.0.18
release version:
    #!/usr/bin/env bash
    set -euo pipefail

    version="{{ version }}"
    version="${version#v}"
    if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      echo "error: version must be semver like 0.0.18 (got '{{ version }}')" >&2
      exit 1
    fi
    if [[ -n "$(git status --porcelain)" ]]; then
      echo "error: working tree is dirty; commit or stash first" >&2
      exit 1
    fi
    branch="$(git branch --show-current)"
    if [[ "$branch" != "main" ]]; then
      echo "error: releases must be cut from main (currently '$branch')" >&2
      exit 1
    fi
    if git rev-parse "v$version" >/dev/null 2>&1; then
      echo "error: tag v$version already exists" >&2
      exit 1
    fi
    current_version="$(awk '
      /^\[package\]$/ { in_package = 1 }
      in_package && /^version = / { gsub(/"/, "", $3); print $3; exit }
    ' Cargo.toml)"
    if [[ "$current_version" == "$version" ]]; then
      echo "error: version $version is already current" >&2
      exit 1
    fi

    remote="${RELEASE_REMOTE:-}"
    if [[ -z "$remote" ]]; then
      remote="$(git config --get "branch.$branch.remote" || true)"
    fi
    if [[ -z "$remote" || "$remote" == "." ]]; then
      if git remote get-url origin >/dev/null 2>&1; then
        remote="origin"
      elif git remote get-url upstream >/dev/null 2>&1; then
        remote="upstream"
      else
        echo "error: no push remote found; set RELEASE_REMOTE" >&2
        exit 1
      fi
    fi
    set +e
    remote_tag="$(git ls-remote --exit-code --tags "$remote" "refs/tags/v$version" 2>&1)"
    remote_tag_status=$?
    set -e
    case "$remote_tag_status" in
      0)
        echo "error: remote tag v$version already exists on $remote" >&2
        exit 1
        ;;
      2) ;;
      *)
        echo "error: could not check tag v$version on $remote: $remote_tag" >&2
        exit 1
        ;;
    esac

    temporary_directory="$(mktemp -d)"
    trap 'rm -rf "$temporary_directory"' EXIT

    update_package_version() {
      local manifest="$1"
      local temporary="$temporary_directory/${manifest//\//_}"
      awk -v version="$version" '
        /^\[package\]$/ { in_package = 1 }
        in_package && !updated && /^version = / {
          print "version = \"" version "\""
          updated = 1
          next
        }
        { print }
        END { if (!updated) exit 1 }
      ' "$manifest" >"$temporary"
      cat "$temporary" >"$manifest"
    }

    update_version_assertion() {
      local path="$1"
      local marker="$2"
      local temporary="$temporary_directory/${path//\//_}"
      awk -v version="$version" -v marker="$marker" '
        index($0, marker) {
          if (match($0, /"[0-9]+\.[0-9]+\.[0-9]+"/)) {
            before = substr($0, 1, RSTART - 1)
            after = substr($0, RSTART + RLENGTH)
            print before "\"" version "\"" after
            updated = 1
            next
          }
        }
        { print }
        END { if (!updated) exit 1 }
      ' "$path" >"$temporary"
      cat "$temporary" >"$path"
    }

    for manifest in \
      Cargo.toml \
      crates/ramo-core/Cargo.toml \
      crates/ramo-github/Cargo.toml \
      crates/ramo-mobile/Cargo.toml \
      crates/ramo-server/Cargo.toml \
      crates/uniffi-bindgen/Cargo.toml
    do
      update_package_version "$manifest"
    done
    update_version_assertion crates/ramo-mobile/src/lib.rs 'super::core_version()'
    update_version_assertion \
      android/app/src/test/kotlin/io/github/carlosarraes/ramo/data/RamoBridgeTest.kt \
      'bridge.coreVersion()'

    cargo check --workspace --quiet
    just check

    git add \
      Cargo.toml Cargo.lock \
      crates/ramo-core/Cargo.toml \
      crates/ramo-github/Cargo.toml \
      crates/ramo-mobile/Cargo.toml \
      crates/ramo-server/Cargo.toml \
      crates/uniffi-bindgen/Cargo.toml \
      crates/ramo-mobile/src/lib.rs \
      android/app/src/test/kotlin/io/github/carlosarraes/ramo/data/RamoBridgeTest.kt
    if git diff --cached --quiet; then
      echo "error: version $version is already current; refusing an empty release" >&2
      exit 1
    fi
    git commit -m "chore: release version $version"
    git tag -a "v$version" -m "v$version"
    git push "$remote" HEAD
    git push "$remote" "v$version"
    echo "Pushed v$version; GitHub Actions will build and publish the release."
