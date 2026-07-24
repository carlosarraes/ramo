# Installer Version Reporting and v0.0.12 Release Design

## Goal

Make the Unix curl installer say exactly which Ramo version it is downloading,
then publish the completed pull-request context feature as `v0.0.12`.

## Installer behavior

When `install.sh` receives an explicit version argument, it will keep using that
tag directly and will print it before downloading.

When the normal curl command uses the default `latest` value, the installer will
first query GitHub's latest-release API, extract the release tag, and fail with
an actionable message if no tag can be resolved. It will then construct the
download URL from that exact tag instead of the moving `latest/download` URL.
The normal output will identify the version and target before the archive
download.

An explicit-version dry run remains network-free. A default dry run will report
`latest` without resolving it so installer selection tests and diagnostic use do
not unexpectedly contact GitHub.

This change applies to the Unix curl installer only. The PowerShell installer is
outside this request.

## Verification

Installer tests will cover:

- an explicit version being printed before its pinned download URL;
- a default install resolving and printing a fake latest GitHub tag;
- a missing latest tag producing an actionable failure;
- the existing OS/architecture, stdin execution, and legacy-binary behavior.

The complete Rust test suite, clippy with warnings denied, formatting check, and
release build will run before integration.

## Release and integration

The release is warranted because lazy unchanged-context expansion in GitHub pull
request reviews is new user-facing functionality. The package and lockfile
versions will move from `0.0.11` to `0.0.12` in a separate release commit.

After verification, `main` will be fast-forwarded to the feature branch, pushed
to `upstream`, tagged `v0.0.12`, and the tag pushed. The existing GitHub Actions
release workflow will build and publish the platform archives. The owned feature
worktree and merged feature branch will be removed only after the fast-forward
and pushes succeed.
