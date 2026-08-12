# `just release` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a safe `just release X.Y.Z` command and use it to publish Ramo `v0.0.18`.

**Architecture:** Keep the workflow inline in the root `justfile`, adapting the validated release recipe from sibling Rust repositories for Ramo's six-package workspace. A shell regression test exercises the non-mutating safety guards, while the first real release verifies coordinated version editing, commit/tag creation, remote pushes, and the tag-triggered GitHub release workflow end to end.

**Tech Stack:** Just, Bash, Git, Cargo, GitHub Actions, GitHub CLI

## Global Constraints

- Require an explicit full version and accept `X.Y.Z` or `vX.Y.Z`.
- Permit caller-selected patch, minor, or major versions without inferring an increment.
- Require a clean `main` working tree before mutation.
- Update all six Cargo package versions and both exact mobile version assertions.
- Run CI-equivalent Rust checks before committing or tagging.
- Create an annotated `vX.Y.Z` tag and push `HEAD` before the tag.
- Use the current branch remote, then `origin`, then `upstream`, unless `RELEASE_REMOTE` is set.
- Leave edits inspectable and do not push if verification fails.

---

## File Structure

- `justfile`: Owns reusable CI checks and the complete release workflow.
- `tests/release_recipe.sh`: Exercises recipe discovery, semver validation, dirty-tree rejection, wrong-branch rejection, and existing-tag rejection without publishing.
- Six Cargo manifests, `Cargo.lock`, and two version assertions: Mutated by the recipe, not hard-coded to the first release in the automation commit.

### Task 1: Add Tested Release Automation

**Files:**
- Modify: `justfile`
- Create: `tests/release_recipe.sh`
- Restore before RED: `Cargo.toml`, `Cargo.lock`, `crates/ramo-core/Cargo.toml`, `crates/ramo-github/Cargo.toml`, `crates/ramo-mobile/Cargo.toml`, `crates/ramo-server/Cargo.toml`, `crates/uniffi-bindgen/Cargo.toml`, `crates/ramo-mobile/src/lib.rs`, `android/app/src/test/kotlin/io/github/carlosarraes/ramo/data/RamoBridgeTest.kt`

**Interfaces:**
- Consumes: `just release <version>` and optional `RELEASE_REMOTE`.
- Produces: release commit `chore: release version X.Y.Z`, annotated tag `vX.Y.Z`, pushed branch and tag.

- [ ] **Step 1: Restore the manually started release bump**

Use `apply_patch` to reverse only the current uncommitted version edits:

```diff
*** Begin Patch
*** Update File: Cargo.toml
@@
-version = "0.0.18"
+version = "0.0.17"
*** Update File: crates/ramo-core/Cargo.toml
@@
-version = "0.0.18"
+version = "0.0.17"
*** Update File: crates/ramo-github/Cargo.toml
@@
-version = "0.0.18"
+version = "0.0.17"
*** Update File: crates/ramo-mobile/Cargo.toml
@@
-version = "0.0.18"
+version = "0.0.17"
*** Update File: crates/ramo-server/Cargo.toml
@@
-version = "0.0.18"
+version = "0.0.17"
*** Update File: crates/uniffi-bindgen/Cargo.toml
@@
-version = "0.0.18"
+version = "0.0.17"
*** Update File: crates/ramo-mobile/src/lib.rs
@@
-        assert_eq!(super::core_version(), "0.0.18");
+        assert_eq!(super::core_version(), "0.0.17");
*** Update File: android/app/src/test/kotlin/io/github/carlosarraes/ramo/data/RamoBridgeTest.kt
@@
-        assertEquals("0.0.18", bridge.coreVersion())
+        assertEquals("0.0.17", bridge.coreVersion())
*** Update File: Cargo.lock
@@
 name = "ramo"
-version = "0.0.18"
+version = "0.0.17"
@@
 name = "ramo-core"
-version = "0.0.18"
+version = "0.0.17"
@@
 name = "ramo-github"
-version = "0.0.18"
+version = "0.0.17"
@@
 name = "ramo-mobile"
-version = "0.0.18"
+version = "0.0.17"
@@
 name = "ramo-server"
-version = "0.0.18"
+version = "0.0.17"
@@
 name = "uniffi-bindgen"
-version = "0.0.18"
+version = "0.0.17"
*** End Patch
```

Confirm `git status --short` is clean before adding automation; do not alter the
four already committed feature/design commits.

- [ ] **Step 2: Write the failing guard regression**

Create executable `tests/release_recipe.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT

if ! just --working-directory "$root" --list | grep -q '^    release version'; then
  echo "release recipe is missing" >&2
  exit 1
fi
```

This intentionally requires the missing interface; after observing RED, expand
it to the complete guard matrix in Step 5.

- [ ] **Step 3: Run the regression and verify RED**

Run:

```bash
bash tests/release_recipe.sh
```

Expected: exit 1 because the script detects that `release` is absent. This proves the regression observes the missing interface.

- [ ] **Step 4: Add `check` and the release recipe**

Extend `justfile` with:

```just
# Run the same Rust checks expected by CI.
check:
    cargo fmt --all -- --check
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    cargo test --locked --workspace --all-targets --all-features
    bash tests/release_recipe.sh

# Cut an exact release, then let the tag-triggered workflow publish artifacts.
# Usage: just release 0.0.18
release version:
    #!/usr/bin/env bash
    set -euo pipefail

    version="{{version}}"
    version="${version#v}"
    if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      echo "error: version must be semver like 0.0.18 (got '{{version}}')" >&2
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
      mv "$temporary" "$manifest"
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
      mv "$temporary" "$path"
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
```

- [ ] **Step 5: Complete the guard regression and verify GREEN**

Replace the temporary failing body in `tests/release_recipe.sh` with:

```bash
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

recipes="$(just --working-directory "$root" --list)"
grep -q '^    check' <<<"$recipes"
grep -q '^    release version' <<<"$recipes"
assert_fails_with \
  "version must be semver" \
  just --working-directory "$root" release invalid

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
  just --working-directory "$repository" release 9.9.9
git -C "$repository" restore tracked

git -C "$repository" switch --quiet -c feature
assert_fails_with \
  "releases must be cut from main" \
  just --working-directory "$repository" release 9.9.9
git -C "$repository" switch --quiet main

git -C "$repository" tag v9.9.9
assert_fails_with \
  "tag v9.9.9 already exists" \
  just --working-directory "$repository" release 9.9.9
```

The test uses only temporary repositories, local Git operations, and early
guards; it never reaches version editing, verification, commit, or push.

Run:

```bash
bash tests/release_recipe.sh
just --list
```

Expected: the regression exits 0 and the recipe list includes `check` and `release version`.

- [ ] **Step 6: Run repository verification**

Run:

```bash
just check
git diff --check
```

Expected: formatting, Clippy, workspace tests, release guards, and whitespace checks all exit 0.

- [ ] **Step 7: Commit the automation**

```bash
git add justfile tests/release_recipe.sh
git commit -m "feat: automate exact version releases"
```

### Task 2: Publish Version 0.0.18 Through the Recipe

**Files:**
- Modify through recipe: `Cargo.toml`, `Cargo.lock`, `crates/ramo-core/Cargo.toml`, `crates/ramo-github/Cargo.toml`, `crates/ramo-mobile/Cargo.toml`, `crates/ramo-server/Cargo.toml`, `crates/uniffi-bindgen/Cargo.toml`, `crates/ramo-mobile/src/lib.rs`, `android/app/src/test/kotlin/io/github/carlosarraes/ramo/data/RamoBridgeTest.kt`

**Interfaces:**
- Consumes: the clean automation commit and `just release 0.0.18`.
- Produces: pushed release commit and tag, then GitHub Release `v0.0.18`.

- [ ] **Step 1: Run the new release interface**

```bash
just release 0.0.18
```

Expected: the recipe updates all versions, passes `just check`, commits `chore: release version 0.0.18`, tags `v0.0.18`, pushes `main`, and pushes the tag.

- [ ] **Step 2: Verify local and remote Git state**

```bash
test "$(git branch --show-current)" = main
test -z "$(git status --porcelain)"
test "$(git rev-parse HEAD)" = "$(git rev-list -n 1 v0.0.18)"
test "$(git rev-parse HEAD)" = "$(git ls-remote upstream refs/heads/main | cut -f1)"
test "$(git rev-parse 'v0.0.18^{}')" = "$(git ls-remote upstream 'refs/tags/v0.0.18^{}' | cut -f1)"
```

Expected: every assertion exits 0.

- [ ] **Step 3: Monitor the release workflow**

```bash
run_id="$(gh run list \
  --workflow Release \
  --branch v0.0.18 \
  --limit 1 \
  --json databaseId \
  --jq '.[0].databaseId')"
test -n "$run_id"
gh run watch "$run_id" --exit-status
```

Expected: all six build jobs and the GitHub Release job succeed.

- [ ] **Step 4: Verify published release assets**

```bash
gh release view v0.0.18 --json tagName,isDraft,isPrerelease,url,assets
```

Expected: a published, non-draft, non-prerelease `v0.0.18` release with Linux, macOS, and Windows archives for x86_64 and aarch64.
