# Ramo Android Network and Authentication Error Design

**Date:** 2026-07-28
**Status:** Approved for implementation planning

## Summary

Ramo's Android client currently reaches GitHub through `reqwest` with rustls's
Android platform certificate verifier, but the application never initializes
that verifier or packages its JVM helper. The first real HTTPS request can
therefore panic with an internal message such as `event loop thread panicked`.
The Kotlin UI then exposes that implementation detail because unknown
exceptions fall back to `Throwable.message`.

Ramo will initialize the Android platform verifier once at process startup,
before any foreground or WorkManager network operation. Authentication and API
failures will be mapped to explicit application states so internal panic text is
never rendered. A token accepted by GitHub will remain encrypted on the device
when organization access is unavailable, allowing the user to retry after an
administrator approves it or to sign out deliberately.

## Goals

- Make the first and every subsequent GitHub HTTPS request work on Android.
- Continue using Android's system trust store rather than shipping a separate
  certificate-root bundle.
- Never show Rust, JNI, UniFFI, coroutine, or HTTP-client internals to the user.
- Distinguish invalid credentials, unavailable organization access, rate
  limiting, connectivity failures, and unexpected failures where GitHub's
  response permits it.
- Preserve an encrypted, otherwise valid token while organization approval or
  repository authorization is unresolved.
- Give retryable states an obvious **Retry** action and always provide **Sign
  out** when a token is retained.
- Cover the state transitions with automated tests and prove real TLS traffic
  on an Android device.

## Non-goals

- Automating approval of a fine-grained personal access token.
- Querying GitHub's organization-owner-only pending-token API.
- Claiming with certainty that a token is pending when GitHub only reports that
  organization or repository access is unavailable.
- Replacing fine-grained personal access tokens with OAuth or a GitHub App.
- Replacing rustls with the platform's HTTP client or bundling WebPKI roots.
- Building a general crash-reporting or telemetry system.

## Root Cause and Chosen Approach

The `reqwest` `rustls` feature selects `rustls-platform-verifier`. On Android,
that verifier requires a JVM helper artifact and one-time initialization with
the application context before certificate verification. Neither is present in
the current app. Its internal global verifier is consequently unset when
`MobileSession.viewer()` makes the first HTTPS request.

The chosen fix is to keep the platform verifier and initialize it correctly.
This preserves Android's trust decisions, including system updates and managed
device certificates. Two alternatives were rejected:

1. Shipping a static WebPKI root set would avoid JNI initialization but would
   stop following the device trust store and behave poorly on managed devices.
2. Catching or renaming the current exception would hide the symptom while
   leaving HTTPS unusable.

## Native TLS Bootstrap

The Android app will include the Kotlin/JVM verifier component associated with
the Cargo-resolved `rustls-platform-verifier-android` package. Gradle will locate
that exact package through `cargo metadata`, preventing the Kotlin helper from
silently drifting away from the Rust crate version.

A small JNI entry point in `ramo-mobile` will accept the Android application
context and initialize `rustls_platform_verifier` exactly once. An Android
`Application` subclass will load `libramo_mobile` and invoke the entry point from
`onCreate`. The manifest will select that application class. This process-level
bootstrap happens before `MainActivity`, ViewModels, or a notification worker
can create a `MobileSession`.

Initialization is idempotent. A bootstrap failure becomes a controlled native
initialization error; networking remains disabled and the UI shows a safe
restart-oriented message instead of attempting a request that can panic.

Release shrinking rules will retain the verifier's JVM classes because their
use is visible through JNI rather than ordinary Kotlin references.

## Authentication and Access States

Authentication has two distinct questions:

1. Does GitHub recognize the token and identify its owner?
2. Can that token currently access the organization repositories needed by
   Ramo?

The UI will model them separately. The auth state machine becomes:

- **Signed out**: no retained token.
- **Validating**: checking the submitted or restored token.
- **Signed in**: GitHub recognized the token and normal inbox loading can run.
- **Access unavailable**: GitHub recognized the token, but an organization or
  repository operation was forbidden. The screen explains that approval may
  still be pending and offers **Retry** and **Sign out**.
- **Retryable failure**: connectivity, TLS/bootstrap, rate-limit, or temporary
  GitHub failure. The token is retained when it has already been accepted and
  the UI offers **Retry** and **Sign out**.
- **Invalid credentials**: GitHub returned an authentication failure. The
  rejected token is removed and the user returns to token entry.

When a new token is submitted, it is stored only after GitHub successfully
identifies the viewer. Once accepted, it is retained across inbox authorization
and network failures. Restoring the app uses the same state machine.

GitHub does not give the token owner a reliable API for reading an
organization's pending approval request; the relevant management API is for
organization owners through GitHub Apps. Ramo will therefore avoid a false
claim. For a forbidden organization operation, the copy will say:

> Organization access isn't active. This token may still be awaiting approval.

An empty, successful inbox remains an empty inbox because it is not proof of a
pending token. Its empty-state help may mention organization approval as a
troubleshooting hint without turning the screen into an error.

## Error Boundary

Rust remains responsible for translating transport and GitHub responses into
stable `MobileError` variants. The mobile boundary will expose categories for:

- invalid credentials;
- organization or repository access unavailable;
- rate limited;
- network unavailable;
- native TLS/bootstrap unavailable;
- stale revision;
- validation failure; and
- unexpected server response.

Kotlin will have one exhaustive user-facing mapper shared by authentication,
inbox, review, and notification paths. It will return presentation data rather
than raw exception strings: message, whether retry is meaningful, and whether
sign-out should be offered. Unknown `Throwable` values map to a generic message
and are logged locally with their technical cause; their messages are never
displayed.

Background notification work uses the same categories but translates them to
WorkManager outcomes: retry temporary failures, stop retrying invalid
credentials, and avoid posting misleading notifications for authorization
failures.

## UI Behavior

The Tokyo Night visual language remains unchanged. Retryable authentication or
access problems use a compact status panel rather than a stack trace or modal
loop. The panel contains:

- a short title;
- one actionable sentence;
- a primary **Retry** button; and
- a secondary **Sign out** button when a token is retained.

The user can leave the message by retrying successfully or signing out. A
successful retry replaces the panel with the inbox. Sign out closes the native
session, removes the encrypted token, and returns to token entry.

## Verification

Implementation follows regression-first testing:

1. Kotlin auth tests prove that an accepted token is retained on forbidden and
   network failures, invalid credentials remove it, retry reuses it without
   requiring another paste, and sign out clears it.
2. Error-mapping tests prove that every typed native error produces approved
   copy and an unknown exception never exposes its message.
3. Rust tests prove `GithubError` categories map to the intended mobile error
   variants.
4. An Android instrumented network smoke test performs a real request to
   `api.github.com` with a deliberately invalid credential and expects the typed
   invalid-credentials result. Reaching that result proves DNS, TLS, JNI
   verifier initialization, HTTP, and UniFFI exception mapping all completed.
5. The signed release APK is installed over the existing app and exercised on
   the physical device. Logcat must contain no verifier or event-loop panic.

No real token is committed, printed, extracted from the device, or embedded in
automated tests.

## Rollout and Compatibility

The change is Android-specific except for stable additions to `ramo-mobile`'s
error surface. Terminal Ramo behavior is unchanged. Existing encrypted tokens
remain compatible and are validated through the corrected startup path after
upgrade. The package name and signing identity remain unchanged so the fixed APK
can be installed in place without losing local drafts or settings.
