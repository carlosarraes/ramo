# Android Network and Authentication Errors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Ramo's Android GitHub HTTPS path reliable and replace leaked runtime failures with retryable, token-safe application states.

**Architecture:** A process-start JNI bootstrap initializes rustls with Android's application context and packages the matching verifier AAR resolved from Cargo metadata. Rust exposes stable mobile error categories; Kotlin maps those categories once into safe presentation data used by authentication, inbox, review, and background polling.

**Tech Stack:** Rust 2024, reqwest 0.13, rustls-platform-verifier 0.7, jni 0.22, UniFFI 0.32, Kotlin 2.3, Jetpack Compose, WorkManager, Android instrumented tests.

## Global Constraints

- Keep Android's system trust store; do not ship a separate certificate-root bundle.
- Initialize the verifier once before any `MobileSession` is created by an activity or worker.
- Never display raw Rust, JNI, UniFFI, coroutine, HTTP-client, or `Throwable.message` text.
- Retain a GitHub-accepted token across organization-access, rate-limit, and connectivity failures.
- Remove the token only for invalid credentials or explicit sign out.
- Describe forbidden organization access as possibly awaiting approval; do not claim certainty.
- An empty successful inbox remains a normal empty state.
- No real token may appear in source, tests, logs, or build artifacts.
- Preserve package `io.github.carlosarraes.ramo`, Android version `0.1.0`, and the existing signing identity.

---

## File Structure

- `crates/ramo-mobile/src/lib.rs`: stable UniFFI error variants and Rust-to-mobile error mapping tests.
- `crates/ramo-mobile/src/android.rs`: Android-only JNI entry point that initializes rustls's platform verifier.
- `crates/ramo-mobile/Cargo.toml`: Android-only `jni` and direct verifier dependencies.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/errors/UserFacingFailure.kt`: the only native/unknown exception-to-copy mapper.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/network/NativeNetworkBootstrap.kt`: loads the native library, owns bootstrap status, and invokes JNI.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/RamoApplication.kt`: performs native bootstrap at process start.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/auth/AuthViewModel.kt`: token validation, retention, retry, and sign-out state machine.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/auth/TokenScreen.kt`: token entry versus retained-token failure panel.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxModels.kt`: typed failure on each inbox tab.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxViewModel.kt`: safe mapping for initial refresh and pagination.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxScreen.kt`: access-unavailable and retry UI.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModel.kt`: replace raw exception fallback with the shared mapper.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/notifications/ReviewNotificationWorker.kt`: shared categories translated into WorkManager outcomes.
- `android/app/src/main/kotlin/io/github/carlosarraes/ramo/MainActivity.kt`: bootstrap-failure gate and retry/sign-out callbacks.
- `android/app/src/main/AndroidManifest.xml`: select `RamoApplication`.
- `android/settings.gradle.kts`: expose Cargo's matching local verifier Maven repository.
- `android/app/build.gradle.kts`: resolve/package the matching verifier AAR and retain JNI verifier classes.
- `android/app/proguard-rules.pro`: verifier keep rule for future minified releases.
- `android/app/src/test/...`: deterministic JVM state/error regression tests.
- `android/app/src/androidTest/kotlin/io/github/carlosarraes/ramo/network/GithubTlsSmokeTest.kt`: real Android HTTPS proof using a deliberately invalid credential.

---

### Task 1: Stable Mobile Error Contract and Safe Kotlin Mapper

**Files:**
- Modify: `crates/ramo-mobile/src/lib.rs`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/errors/UserFacingFailure.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/errors/UserFacingFailureTest.kt`

**Interfaces:**
- Consumes: `GithubErrorKind` and generated `MobileException` subclasses.
- Produces: `MobileError::AccessUnavailable`, `FailureKind`, `UserFacingFailure`, and `Throwable.toUserFacingFailure(fallback)`.

- [ ] **Step 1: Write Rust mapping tests that distinguish forbidden access**

Add a test helper and assertions in `crates/ramo-mobile/src/lib.rs`:

```rust
#[test]
fn maps_github_failures_to_stable_mobile_categories() {
    use ramo_github::{GithubError, GithubErrorKind};

    assert_eq!(
        super::MobileError::from(GithubError::new(
            GithubErrorKind::Forbidden,
            "private detail that must not cross FFI",
        )),
        super::MobileError::AccessUnavailable,
    );
    assert_eq!(
        super::MobileError::from(GithubError::new(
            GithubErrorKind::Transport,
            "private transport detail",
        )),
        super::MobileError::Network,
    );
}
```

Derive `PartialEq, Eq` for `MobileError` so the contract is directly testable.

- [ ] **Step 2: Run the Rust test and verify red**

Run: `cargo test -p ramo-mobile maps_github_failures_to_stable_mobile_categories`

Expected: compilation fails because `AccessUnavailable` does not exist and `MobileError` is not comparable.

- [ ] **Step 3: Rename the mobile forbidden variant without changing GitHub's transport model**

Change the relevant `MobileError` portion to:

```rust
#[derive(Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("GitHub rejected this token")]
    InvalidCredentials,
    #[error("Organization access is not active")]
    AccessUnavailable,
    #[error("GitHub rate limit exceeded")]
    RateLimited,
    #[error("Could not reach GitHub")]
    Network,
    #[error("GitHub returned an unexpected response")]
    Unexpected,
    #[error("The pull request changed while you were reviewing")]
    StaleRevision,
    #[error("This review is not valid")]
    Validation,
}
```

Map `GithubErrorKind::Forbidden` to `Self::AccessUnavailable`. Leave all other mappings intact.

- [ ] **Step 4: Run the Rust test and verify green**

Run: `cargo test -p ramo-mobile maps_github_failures_to_stable_mobile_categories`

Expected: PASS.

- [ ] **Step 5: Write failing Kotlin mapper tests**

Create `UserFacingFailureTest.kt` with:

```kotlin
package io.github.carlosarraes.ramo.errors

import io.github.carlosarraes.ramo.uniffi.MobileException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class UserFacingFailureTest {
    @Test fun forbiddenExplainsApprovalWithoutClaimingItIsPending() {
        val failure = MobileException.AccessUnavailable().toUserFacingFailure("Could not load")
        assertEquals(FailureKind.AccessUnavailable, failure.kind)
        assertEquals(
            "Organization access isn't active. This token may still be awaiting approval.",
            failure.message,
        )
        assertTrue(failure.retryable)
    }

    @Test fun unknownFailureNeverLeaksItsMessage() {
        val failure = IllegalStateException("event loop thread panicked").toUserFacingFailure("Could not sign in to GitHub")
        assertEquals("Could not sign in to GitHub", failure.message)
        assertFalse(failure.message.contains("panicked"))
    }
}
```

- [ ] **Step 6: Run the Kotlin mapper tests and verify red**

Run: `cd android && ./gradlew testDebugUnitTest --tests '*.UserFacingFailureTest'`

Expected: compilation fails because the mapper types do not exist.

- [ ] **Step 7: Implement the exhaustive mapper**

Create `UserFacingFailure.kt`:

```kotlin
package io.github.carlosarraes.ramo.errors

import android.util.Log
import io.github.carlosarraes.ramo.uniffi.MobileException

enum class FailureKind { InvalidCredentials, AccessUnavailable, RateLimited, Network, StaleRevision, Validation, Unexpected }

data class UserFacingFailure(
    val kind: FailureKind,
    val message: String,
    val retryable: Boolean,
)

fun Throwable.toUserFacingFailure(fallback: String): UserFacingFailure {
    val failure = when (this) {
        is MobileException.InvalidCredentials -> UserFacingFailure(FailureKind.InvalidCredentials, "GitHub rejected this token", false)
        is MobileException.AccessUnavailable -> UserFacingFailure(
            FailureKind.AccessUnavailable,
            "Organization access isn't active. This token may still be awaiting approval.",
            true,
        )
        is MobileException.RateLimited -> UserFacingFailure(FailureKind.RateLimited, "GitHub rate limit exceeded; try again later", true)
        is MobileException.Network -> UserFacingFailure(FailureKind.Network, "Could not reach GitHub", true)
        is MobileException.StaleRevision -> UserFacingFailure(FailureKind.StaleRevision, "The pull request changed while you were reviewing", true)
        is MobileException.Validation -> UserFacingFailure(FailureKind.Validation, "GitHub rejected this operation", false)
        is MobileException.Unexpected -> UserFacingFailure(FailureKind.Unexpected, "GitHub returned an unexpected response", true)
        else -> UserFacingFailure(FailureKind.Unexpected, fallback, true)
    }
    if (this !is MobileException) {
        // Local JVM tests do not provide Android's Log implementation.
        runCatching { Log.e("Ramo", "Unexpected application failure", this) }
    }
    return failure
}
```

- [ ] **Step 8: Run focused Rust and Kotlin tests**

Run:

```bash
cargo test -p ramo-mobile
cd android && ./gradlew testDebugUnitTest --tests '*.UserFacingFailureTest'
```

Expected: both commands PASS.

- [ ] **Step 9: Commit the error contract**

```bash
git add crates/ramo-mobile/src/lib.rs android/app/src/main/kotlin/io/github/carlosarraes/ramo/errors/UserFacingFailure.kt android/app/src/test/kotlin/io/github/carlosarraes/ramo/errors/UserFacingFailureTest.kt
git commit -m "fix(android): expose safe mobile failures"
```

---

### Task 2: Retained-token Authentication Retry State

**Files:**
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/auth/AuthViewModel.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/auth/TokenScreen.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/MainActivity.kt`
- Modify: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/auth/AuthViewModelTest.kt`

**Interfaces:**
- Consumes: `UserFacingFailure` and `FailureKind` from Task 1.
- Produces: `AuthState.Failure(failure, tokenRetained)`, `AuthViewModel.retry()`, and token-screen callbacks `onRetry`/`onSignOut`.

- [ ] **Step 1: Add failing tests for restored-token retry, forbidden retention, and invalid-token removal**

Extend `AuthViewModelTest.kt` with fake failures created through `MobileException`:

```kotlin
@Test fun restoredTokenCanRetryWithoutAnotherPaste() = runTest(dispatcher) {
    val store = MemoryTokenStore("saved")
    val authenticator = SequenceAuthenticator(
        mutableListOf(Result.failure(MobileException.Network()), Result.success("carraes")),
    )
    val model = AuthViewModel(store, authenticator)
    model.restore()
    advanceUntilIdle()
    assertTrue((model.state.value as AuthState.Failure).tokenRetained)
    model.retry()
    advanceUntilIdle()
    assertEquals(AuthState.SignedIn("carraes"), model.state.value)
    assertEquals(listOf("saved", "saved"), authenticator.tokens)
}

@Test fun forbiddenTokenIsRetainedForOrganizationApproval() = runTest(dispatcher) {
    val store = MemoryTokenStore()
    val model = AuthViewModel(store, FakeAuthenticator(Result.failure(MobileException.AccessUnavailable())))
    model.validate("candidate-token")
    advanceUntilIdle()
    assertEquals("candidate-token", store.token)
    assertTrue((model.state.value as AuthState.Failure).tokenRetained)
}

@Test fun invalidRestoredTokenIsRemoved() = runTest(dispatcher) {
    val store = MemoryTokenStore("revoked")
    val model = AuthViewModel(store, FakeAuthenticator(Result.failure(MobileException.InvalidCredentials())))
    model.restore()
    advanceUntilIdle()
    assertNull(store.token)
}
```

Add `SequenceAuthenticator` with a `tokens` list and FIFO `Result<String>` answers.
Update the existing generic `Exception("bad token")` fixture to
`MobileException.InvalidCredentials()`; unknown exceptions intentionally map to
the safe fallback rather than being guessed from their text.

- [ ] **Step 2: Run auth tests and verify red**

Run: `cd android && ./gradlew testDebugUnitTest --tests '*.AuthViewModelTest'`

Expected: compilation fails because `AuthState.Failure`, `retry`, and `AccessUnavailable` behavior do not exist.

- [ ] **Step 3: Implement the auth state machine**

Replace `AuthState.Error` with:

```kotlin
data class Failure(
    val failure: UserFacingFailure,
    val tokenRetained: Boolean,
) : AuthState
```

Track the current candidate without exposing it:

```kotlin
private var retryToken: String? = null

fun retry() {
    val token = retryToken ?: tokenStore.read() ?: return signOut()
    validate(token, persist = tokenStore.read() == null)
}
```

At validation start, set `retryToken = token`. On success, write the token,
clear `retryToken`, and enter `SignedIn`. On failure:

```kotlin
val failure = error.toUserFacingFailure("Could not sign in to GitHub")
when (failure.kind) {
    FailureKind.InvalidCredentials -> {
        tokenStore.clear()
        retryToken = null
    }
    FailureKind.AccessUnavailable -> tokenStore.write(token)
    else -> Unit
}
mutableState.value = AuthState.Failure(failure, tokenStore.read() != null)
```

`signOut()` must also clear `retryToken`.

- [ ] **Step 4: Render retained failures without asking for the token again**

Change the `TokenScreen` signature to:

```kotlin
fun TokenScreen(
    state: AuthState,
    onValidate: (String) -> Unit,
    onRetry: () -> Unit,
    onSignOut: () -> Unit,
)
```

When `state is AuthState.Failure && state.tokenRetained`, hide the token field and show its safe message, a full-width **Retry** button, and **Sign out** text button. For a non-retained failure, keep the token field and show only the safe message. Never render an exception value.

Update `MainActivity` to call:

```kotlin
TokenScreen(current, auth::validate, auth::retry, auth::signOut)
```

- [ ] **Step 5: Run auth tests and the Android JVM suite**

Run:

```bash
cd android
./gradlew testDebugUnitTest --tests '*.AuthViewModelTest'
./gradlew testDebugUnitTest
```

Expected: both commands PASS.

- [ ] **Step 6: Commit retained-token retry**

```bash
git add android/app/src/main/kotlin/io/github/carlosarraes/ramo/auth android/app/src/main/kotlin/io/github/carlosarraes/ramo/MainActivity.kt android/app/src/test/kotlin/io/github/carlosarraes/ramo/auth/AuthViewModelTest.kt
git commit -m "fix(android): retain tokens while access is pending"
```

---

### Task 3: Safe Foreground and Background Failure Handling

**Files:**
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxModels.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxViewModel.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxScreen.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModel.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/notifications/ReviewNotificationWorker.kt`
- Modify: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/inbox/InboxViewModelTest.kt`
- Modify: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModelTest.kt`
- Modify: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/notifications/ReviewNotificationWorkerTest.kt`

**Interfaces:**
- Consumes: `UserFacingFailure`, `FailureKind`, and `Throwable.toUserFacingFailure`.
- Produces: `TabState.failure: UserFacingFailure?` and consistent WorkManager outcomes.

- [ ] **Step 1: Write failures-first tests for inbox and review copy**

Add an inbox repository fake that throws `MobileException.AccessUnavailable()` and assert:

```kotlin
val failure = model.state.value.reviewRequests.failure!!
assertEquals(FailureKind.AccessUnavailable, failure.kind)
assertFalse(failure.message.contains("Forbidden"))
```

Add a review repository fake that throws `IllegalStateException("event loop thread panicked")` from `open` and assert:

```kotlin
assertEquals("Could not load this pull request", model.state.value.error)
assertFalse(model.state.value.error!!.contains("panicked"))
```

- [ ] **Step 2: Write background-runner tests for authorization and unknown failures**

Keep `PollFailure.Revoked` clearing the token. Assert `PollFailure.AccessUnavailable` returns `WorkerOutcome.Failure` without clearing it, while `PollFailure.Retryable` returns `Retry` without clearing it.

- [ ] **Step 3: Run the three focused suites and verify red**

Run:

```bash
cd android
./gradlew testDebugUnitTest --tests '*.InboxViewModelTest' --tests '*.ReviewViewModelTest' --tests '*.ReviewNotificationWorkerTest'
```

Expected: FAIL because typed inbox failures and `AccessUnavailable` polling do not exist and review still leaks `Throwable.message`.

- [ ] **Step 4: Replace raw foreground error strings**

Change `TabState.error: String?` to `failure: UserFacingFailure?`. In `InboxViewModel`, map initial refresh failures with:

```kotlin
val failure = error.toUserFacingFailure("Could not load pull requests")
current.copy(
    loading = false,
    failure = if (current.items.isEmpty()) failure else UserFacingFailure(
        failure.kind,
        "Offline · showing last refresh",
        failure.retryable,
    ),
)
```

Use `failure.message` in `InboxScreen`; show **Retry** only when `failure.retryable`. The existing signed-in header keeps **Sign out** visible. Add this normal empty-state hint without changing empty success into an error:

```text
No reviews waiting. If you expected private pull requests, check whether organization access is still awaiting approval.
```

Replace `ReviewViewModel.message` with:

```kotlin
private fun message(error: Throwable) =
    error.toUserFacingFailure("Could not load this pull request").message
```

- [ ] **Step 5: Route worker errors through stable categories**

Rename `PollFailure.Fatal` to `PollFailure.AccessUnavailable`. In `BridgeReviewPoller`, translate `MobileException.AccessUnavailable` to that value, invalid credentials to `Revoked`, rate/network/unexpected to `Retryable`, and unknown exceptions to `Retryable` after local logging. `ReviewNotificationRunner` must not clear the token for access-unavailable or retryable failures.

- [ ] **Step 6: Run focused and full Android JVM tests**

Run:

```bash
cd android
./gradlew testDebugUnitTest --tests '*.InboxViewModelTest' --tests '*.ReviewViewModelTest' --tests '*.ReviewNotificationWorkerTest'
./gradlew testDebugUnitTest
```

Expected: both commands PASS and no production Kotlin source renders `error.message`.

Run: `rg -n 'error\.message|Throwable\.message' android/app/src/main/kotlin`

Expected: no matches.

- [ ] **Step 7: Commit unified error handling**

```bash
git add android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModel.kt android/app/src/main/kotlin/io/github/carlosarraes/ramo/notifications/ReviewNotificationWorker.kt android/app/src/test/kotlin/io/github/carlosarraes/ramo/inbox/InboxViewModelTest.kt android/app/src/test/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModelTest.kt android/app/src/test/kotlin/io/github/carlosarraes/ramo/notifications/ReviewNotificationWorkerTest.kt
git commit -m "fix(android): present actionable github errors"
```

---

### Task 4: Android TLS Bootstrap and Real Network Proof

**Files:**
- Modify: `crates/ramo-mobile/Cargo.toml`
- Modify: `crates/ramo-mobile/src/lib.rs`
- Create: `crates/ramo-mobile/src/android.rs`
- Modify: `android/app/build.gradle.kts`
- Modify: `android/settings.gradle.kts`
- Create: `android/app/proguard-rules.pro`
- Modify: `android/app/src/main/AndroidManifest.xml`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/network/NativeNetworkBootstrap.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/RamoApplication.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/MainActivity.kt`
- Create: `android/app/src/androidTest/kotlin/io/github/carlosarraes/ramo/network/GithubTlsSmokeTest.kt`

**Interfaces:**
- Consumes: Android `Context`, `rustls_platform_verifier::android::init_with_env`, and `MobileSession`.
- Produces: `NativeNetworkBootstrap.initialize(Context): BootstrapStatus`, process-wide `BootstrapStatus`, and the JNI symbol `Java_io_github_carlosarraes_ramo_network_NativeNetworkBootstrap_initializeNative`.

- [ ] **Step 1: Add the Android-only native dependencies**

Append to `crates/ramo-mobile/Cargo.toml`:

```toml
[target.'cfg(target_os = "android")'.dependencies]
jni = "0.22.4"
rustls-platform-verifier = "0.7.0"
```

Declare `#[cfg(target_os = "android")] mod android;` in `lib.rs`.

- [ ] **Step 2: Implement the panic-safe JNI initializer**

Create `crates/ramo-mobile/src/android.rs`:

```rust
use jni::EnvUnowned;
use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::{JClass, JObject};
use jni::sys::{JNI_TRUE, jboolean};

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_carlosarraes_ramo_network_NativeNetworkBootstrap_initializeNative<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    context: JObject<'caller>,
) -> jboolean {
    unowned_env
        .with_env(|env| {
            rustls_platform_verifier::android::init_with_env(env, context)?;
            Ok(JNI_TRUE)
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}
```

The JNI error policy catches panics and throws a Java runtime exception rather than unwinding across FFI.

- [ ] **Step 3: Make Gradle package the Cargo-matched verifier AAR**

Find the package named `rustls-platform-verifier-android`, resolve its
`manifest_path`, and derive its `maven` directory. Add this helper at the top of
`android/settings.gradle.kts`:

```kotlin
import groovy.json.JsonSlurper

@Suppress("UNCHECKED_CAST")
fun rustlsVerifierMavenDirectory(): File {
    val metadata = providers.exec {
        workingDir = File(settingsDir, "..")
        commandLine(
            "cargo", "metadata", "--format-version", "1",
            "--filter-platform", "aarch64-linux-android",
        )
    }.standardOutput.asText.get()
    val root = JsonSlurper().parseText(metadata) as Map<String, Any?>
    val packages = root.getValue("packages") as List<Map<String, Any?>>
    val manifestPath = packages
        .singleOrNull { it["name"] == "rustls-platform-verifier-android" }
        ?.get("manifest_path") as? String
        ?: error("Cargo metadata does not contain rustls-platform-verifier-android")
    return File(File(manifestPath).parentFile, "maven")
}
```

Then extend the existing repository block:

```kotlin
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        maven {
            url = uri(rustlsVerifierMavenDirectory())
            metadataSources { artifact() }
        }
    }
}
```

Finally add to `android/app/build.gradle.kts`:

```kotlin
implementation("rustls:rustls-platform-verifier:latest.release")
```

The repository lookup must fail the build with a clear message if Cargo metadata does not contain the package. Configure release builds with:

```kotlin
proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
```

Create `proguard-rules.pro` with:

```text
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
```

- [ ] **Step 4: Create a process-start bootstrap with controlled failure**

Create `NativeNetworkBootstrap.kt`:

```kotlin
package io.github.carlosarraes.ramo.network

import android.content.Context
import android.util.Log

sealed interface BootstrapStatus {
    data object Ready : BootstrapStatus
    data object Failed : BootstrapStatus
}

object NativeNetworkBootstrap {
    init { System.loadLibrary("ramo_mobile") }

    @Volatile var status: BootstrapStatus = BootstrapStatus.Failed
        private set

    @JvmStatic private external fun initializeNative(context: Context): Boolean

    fun initialize(context: Context): BootstrapStatus {
        status = try {
            if (initializeNative(context.applicationContext)) BootstrapStatus.Ready else BootstrapStatus.Failed
        } catch (error: Throwable) {
            Log.e("Ramo", "Native TLS initialization failed", error)
            BootstrapStatus.Failed
        }
        return status
    }
}
```

Create `RamoApplication.kt`:

```kotlin
package io.github.carlosarraes.ramo

import android.app.Application
import io.github.carlosarraes.ramo.network.NativeNetworkBootstrap

class RamoApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        NativeNetworkBootstrap.initialize(this)
    }
}
```

Set `android:name=".RamoApplication"` on the manifest's `<application>`.

At the top of `MainActivity`'s Compose content, gate normal auth UI on `NativeNetworkBootstrap.status`. A failed status renders the safe message **Ramo couldn't initialize secure networking. Restart the app and try again.** It must not construct an authenticator session or start notification scheduling.

- [ ] **Step 5: Add a real TLS instrumented regression test**

Create `GithubTlsSmokeTest.kt`:

```kotlin
package io.github.carlosarraes.ramo.network

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.carlosarraes.ramo.uniffi.MobileException
import io.github.carlosarraes.ramo.uniffi.MobileSession
import kotlin.test.Test
import kotlin.test.assertFailsWith
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class GithubTlsSmokeTest {
    @Test fun githubHandshakeReachesTypedAuthenticationFailure() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        check(NativeNetworkBootstrap.initialize(context) == BootstrapStatus.Ready)
        val session = MobileSession("deliberately_invalid_ramo_test_token")
        try {
            assertFailsWith<MobileException.InvalidCredentials> { session.viewer() }
        } finally {
            session.close()
        }
    }
}
```

This test uses no secret. A typed 401 proves native loading, JNI initialization, Android certificate verification, DNS, TLS, HTTP, and UniFFI error mapping completed.

- [ ] **Step 6: Build and run the Android regression suites**

Run:

```bash
cargo fmt --check
cargo test -p ramo-mobile
cd android
./gradlew testDebugUnitTest
./gradlew assembleRelease
./gradlew connectedDebugAndroidTest
```

Expected: all commands PASS. The connected test reports `GithubTlsSmokeTest` passing rather than a panic.

- [ ] **Step 7: Inspect the packaged verifier and device logs**

Run:

```bash
unzip -l android/app/build/outputs/apk/release/app-release.apk | rg 'ramo_mobile|classes.dex'
adb install -r android/app/build/outputs/apk/release/app-release.apk
adb logcat -c
adb shell am force-stop io.github.carlosarraes.ramo
adb shell monkey -p io.github.carlosarraes.ramo 1
adb logcat -d | rg -i 'ramo|rustls|event loop|panic|fatal exception'
```

Expected: the APK contains `lib/arm64-v8a/libramo_mobile.so`; install succeeds; app launch shows no rustls, event-loop, panic, or fatal-exception error.

- [ ] **Step 8: Commit Android TLS initialization**

```bash
git add crates/ramo-mobile/Cargo.toml crates/ramo-mobile/src/lib.rs crates/ramo-mobile/src/android.rs android/app/build.gradle.kts android/settings.gradle.kts android/app/proguard-rules.pro android/app/src/main/AndroidManifest.xml android/app/src/main/kotlin/io/github/carlosarraes/ramo/network android/app/src/main/kotlin/io/github/carlosarraes/ramo/RamoApplication.kt android/app/src/main/kotlin/io/github/carlosarraes/ramo/MainActivity.kt android/app/src/androidTest/kotlin/io/github/carlosarraes/ramo/network/GithubTlsSmokeTest.kt Cargo.lock
git commit -m "fix(android): initialize platform tls verifier"
```

---

### Task 5: Full Verification and Integration

**Files:**
- Modify only files required by formatter or a demonstrated test failure.

**Interfaces:**
- Consumes: all deliverables from Tasks 1-4.
- Produces: a verified feature branch ready for fast-forward integration.

- [ ] **Step 1: Run repository-wide Rust verification**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all commands exit 0 with no warnings.

- [ ] **Step 2: Run Android verification**

Run:

```bash
cd android
./gradlew testDebugUnitTest connectedDebugAndroidTest assembleRelease
```

Expected: unit and instrumented tests PASS and the signed release APK builds.

- [ ] **Step 3: Audit forbidden leaks and secrets**

Run:

```bash
rg -n 'error\.message|Throwable\.message|event loop thread panicked' android/app/src/main/kotlin
rg -n 'github_pat_|ghp_' crates android --glob '!**/build/**'
```

Expected: the first command has no production-code matches. The second finds no real credential; only the deliberately invalid test literal is acceptable.

- [ ] **Step 4: Review the branch diff and working tree**

Run:

```bash
git diff --check main...HEAD
git status --short
git log --oneline main..HEAD
```

Expected: no whitespace errors, no unintended files, and four focused implementation commits. Preserve `docs/superpowers/plans/2026-07-27-github-comment-import.md` in the main worktree.

- [ ] **Step 5: Fast-forward locally after verification**

Return to the main worktree, verify it has no overlapping changes, run `git merge --ff-only <feature-branch>`, and delete the feature worktree and branch. Do not push or release unless the user requests it.
