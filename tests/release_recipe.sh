#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT

assert_fails_with() {
  local expected="$1"
  shift
  local output
  if output="$("$@" 2>&1)"; then
    echo "expected command to fail: $*" >&2
    exit 1
  fi
  if ! grep -Fq "$expected" <<<"$output"; then
    echo "missing failure text '$expected':" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

recipes="$(just --justfile "$root/justfile" --working-directory "$root" --list)"
grep -q '^    check' <<<"$recipes"
grep -q '^    release version' <<<"$recipes"
assert_fails_with \
  "version must be semver" \
  just --justfile "$root/justfile" --working-directory "$root" release invalid

repository="$temporary/repository"
mkdir -p "$repository"
cp "$root/justfile" "$repository/justfile"
git -C "$repository" init -b main --quiet
git -C "$repository" config user.name "Ramo Release Test"
git -C "$repository" config user.email "release-test@example.invalid"
printf 'clean\n' >"$repository/tracked"
git -C "$repository" add justfile tracked
git -C "$repository" commit --quiet -m fixture

printf 'dirty\n' >>"$repository/tracked"
assert_fails_with \
  "working tree is dirty" \
  just --justfile "$repository/justfile" --working-directory "$repository" release 9.9.9
git -C "$repository" restore tracked

git -C "$repository" switch --quiet -c feature
assert_fails_with \
  "releases must be cut from main" \
  just --justfile "$repository/justfile" --working-directory "$repository" release 9.9.9
git -C "$repository" switch --quiet main

git -C "$repository" tag v9.9.9
assert_fails_with \
  "tag v9.9.9 already exists" \
  just --justfile "$repository/justfile" --working-directory "$repository" release 9.9.9
