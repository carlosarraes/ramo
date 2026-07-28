# Android Shell, Authentication, Inbox, and Notifications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce an installable Android APK with secure personal-token entry, Rust/UniFFI connectivity, the two-tab Tokyo Night PR inbox, and periodic review-request notifications.

**Architecture:** Add a single-activity Jetpack Compose app and an arm64 `ramo-mobile` Rust `cdylib`. Generate Kotlin bindings from UniFFI library metadata and keep calls coarse; Kotlin owns lifecycle, Keystore, WorkManager, and UI while Rust owns GitHub operations. Use a manually wired repository/ViewModel graph rather than adding a dependency-injection framework.

**Tech Stack:** Android Gradle Plugin 9.2.1, Gradle 9.4.1, JDK 17, Kotlin 2.3.21, Compose BOM 2026.06.00, compile/target SDK 36, min SDK 28, NDK 28.2.13676358, UniFFI 0.32.0, cargo-ndk 4.1.2.

## Global Constraints

- Package name is `io.github.carlosarraes.ramo`; display name is `Ramo`.
- v1 ships only `arm64-v8a`, matching the connected SM-S928B.
- No token, signing key, `local.properties`, Android SDK, generated bindings, or built APK is committed.
- Fine-grained token and drafts use Android Keystore-backed AES-256-GCM encryption at rest.
- Rust bridge calls run on `Dispatchers.IO`, never the main thread.
- The app uses the approved Tokyo Night palette and compact, code-first visual language.
- Notifications are approximate periodic checks and never claim real-time delivery.

---

### Task 1: Bootstrap reproducible Android tooling and a Compose smoke app

**Files:**
- Modify: `.gitignore`
- Create: `.tool-versions`
- Create: `scripts/bootstrap-android.sh`
- Create: `android/settings.gradle.kts`
- Create: `android/build.gradle.kts`
- Create: `android/gradle.properties`
- Create: `android/gradle/libs.versions.toml`
- Create: `android/gradle/wrapper/gradle-wrapper.properties`
- Create: `android/gradlew`
- Create: `android/gradlew.bat`
- Create: `android/gradle/wrapper/gradle-wrapper.jar`
- Create: `android/app/build.gradle.kts`
- Create: `android/app/src/main/AndroidManifest.xml`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/MainActivity.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/ui/theme/RamoTheme.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/SmokeTest.kt`

**Interfaces:**
- Consumes: asdf-installed `temurin-17.0.19+10` and Android command-line tools archive `15859902`.
- Produces: `./android/gradlew :app:testDebugUnitTest` and a debug APK for API 28+ arm64 devices.

- [ ] **Step 1: Add a failing JVM smoke test**

```kotlin
package io.github.carlosarraes.ramo

import kotlin.test.Test
import kotlin.test.assertEquals

class SmokeTest {
    @Test fun appNameIsRamo() = assertEquals("Ramo", BuildConfig.APP_NAME)
}
```

- [ ] **Step 2: Bootstrap the pinned SDK and Gradle wrapper**

Set `.tool-versions` to:

```text
java temurin-17.0.19+10
```

Create a shell script that downloads `commandlinetools-linux-15859902_latest.zip`, verifies SHA-256 `4e4c464f145a7512b57d088ac6c278c03c9eea610886b35a5e0804e74eedf583`, installs it under `${XDG_DATA_HOME:-$HOME/.local/share}/android-sdk`, accepts licenses, and installs:

```text
platforms;android-36
build-tools;36.0.0
ndk;28.2.13676358
platform-tools
```

The script writes only `android/local.properties` with `sdk.dir=<absolute path>` inside the repository. Add `android/local.properties`, `android/.gradle/`, `android/**/build/`, `target/`, and generated UniFFI output to `.gitignore`.

The same script runs `rustup target add aarch64-linux-android` and installs `cargo-ndk` only when `cargo ndk --version` is absent or not `4.1.2`, using `cargo install cargo-ndk --version 4.1.2 --locked`.

Generate the Gradle 9.4.1 wrapper using the official distribution URL `https://services.gradle.org/distributions/gradle-9.4.1-bin.zip` and commit the wrapper files.

- [ ] **Step 3: Create the pinned Compose project**

Use version-catalog entries for AGP `9.2.1`, Kotlin `2.3.21`, Compose BOM `2026.06.00`, Activity Compose `1.12.4`, Lifecycle `2.10.0`, and WorkManager `2.11.0`. Configure:

```kotlin
android {
    namespace = "io.github.carlosarraes.ramo"
    compileSdk = 36
    defaultConfig {
        applicationId = "io.github.carlosarraes.ramo"
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0-dev"
        buildConfigField("String", "APP_NAME", "\"Ramo\"")
        ndk { abiFilters += "arm64-v8a" }
    }
    buildFeatures { buildConfig = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
```

`MainActivity` uses `ComponentActivity`, `setContent`, and a temporary `Text("ramo")` inside `RamoTheme`.

- [ ] **Step 4: Encode the approved theme**

Define exact Compose colors:

```kotlin
val Background = Color(0xFF1A1B26)
val Surface = Color(0xFF24283B)
val TextPrimary = Color(0xFFC0CAF5)
val TextMuted = Color(0xFF565F89)
val Blue = Color(0xFF7AA2F7)
val Cyan = Color(0xFF7DCFFF)
val Green = Color(0xFF9ECE6A)
val Red = Color(0xFFF7768E)
val Amber = Color(0xFFE0AF68)
val Purple = Color(0xFFBB9AF7)
```

Use a dark-only Material color scheme and disable dynamic color so the approved palette remains stable.

- [ ] **Step 5: Run the first Android gate**

Run: `scripts/bootstrap-android.sh && cd android && ./gradlew :app:testDebugUnitTest :app:assembleDebug`

Expected: PASS and `android/app/build/outputs/apk/debug/app-debug.apk` exists.

- [ ] **Step 6: Commit the Android foundation**

```bash
git add .gitignore .tool-versions scripts/bootstrap-android.sh android
git commit -m "feat: bootstrap ramo android app"
```

### Task 2: Compile and call the UniFFI Rust bridge

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/ramo-mobile/Cargo.toml`
- Create: `crates/ramo-mobile/src/lib.rs`
- Create: `crates/ramo-mobile/uniffi.toml`
- Create: `crates/uniffi-bindgen/Cargo.toml`
- Create: `crates/uniffi-bindgen/src/main.rs`
- Modify: `android/app/build.gradle.kts`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/data/RamoBridge.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/data/RamoBridgeTest.kt`

**Interfaces:**
- Consumes: `ramo-core`, `ramo-github`, runtime token, cargo-ndk.
- Produces: `MobileSession`, generated Kotlin bindings, and `RamoBridge` coroutine wrapper.

- [ ] **Step 1: Write the failing bridge test against an interface**

```kotlin
interface RamoBridge {
    suspend fun coreVersion(): String
}

class RamoBridgeTest {
    @Test fun reportsCoreVersion() = runTest {
        val bridge = NativeRamoBridge()
        assertEquals("0.0.15", bridge.coreVersion())
    }
}
```

- [ ] **Step 2: Add the mobile and bindgen crates**

Add both workspace members. `ramo-mobile` is:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
ramo-core = { path = "../ramo-core" }
ramo-github = { path = "../ramo-github" }
uniffi = "0.32.0"
```

Use proc macros and library-mode generation:

```rust
uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
```

The bindgen binary contains exactly:

```rust
fn main() { uniffi::uniffi_bindgen_main() }
```

Configure `crates/ramo-mobile/uniffi.toml` exactly:

```toml
[bindings.kotlin]
package_name = "io.github.carlosarraes.ramo.uniffi"
android = true
kotlin_target_version = "2.3.21"
```

Add `net.java.dev.jna:jna:5.12.0@aar` and `org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2` to the Android app because generated UniFFI Kotlin requires JNA and coroutines.

- [ ] **Step 3: Wire deterministic Gradle tasks**

Register `buildRustDebug` and `generateUniFfiDebug` tasks. The Rust task runs:

```text
cargo ndk -t arm64-v8a -o android/app/build/generated/jniLibs/debug build -p ramo-mobile
```

The binding task runs:

```text
cargo run -p uniffi-bindgen -- generate --library target/aarch64-linux-android/debug/libramo_mobile.so --language kotlin --out-dir android/app/build/generated/source/uniffi/debug
```

Add generated Kotlin and JNI directories to the debug source set; make Kotlin compilation depend on both tasks.

- [ ] **Step 4: Implement the coroutine wrapper**

```kotlin
class NativeRamoBridge(
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) : RamoBridge {
    override suspend fun coreVersion(): String = withContext(dispatcher) {
        uniffi.ramo_mobile.coreVersion()
    }
}
```

- [ ] **Step 5: Run Rust, bindings, and JVM tests**

Run: `cargo test -p ramo-mobile && cd android && ./gradlew :app:testDebugUnitTest :app:assembleDebug`

Expected: PASS and the APK contains `lib/arm64-v8a/libramo_mobile.so`.

- [ ] **Step 6: Commit the bridge**

```bash
git add Cargo.toml Cargo.lock crates/ramo-mobile crates/uniffi-bindgen android/app/build.gradle.kts android/app/src
git commit -m "feat: bridge rust into android"
```

### Task 3: Store and validate the personal token securely

**Files:**
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/security/EncryptedBlobStore.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/security/TokenStore.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/auth/AuthViewModel.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/auth/TokenScreen.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/auth/AuthViewModelTest.kt`
- Create: `android/app/src/androidTest/kotlin/io/github/carlosarraes/ramo/security/TokenStoreTest.kt`
- Modify: `crates/ramo-mobile/src/lib.rs`

**Interfaces:**
- Consumes: pasted fine-grained token and `GithubClient::viewer`.
- Produces: authenticated `MobileSession`, encrypted token file, sign-out deletion.

- [ ] **Step 1: Write failing ViewModel state tests**

Test these exact states: `SignedOut`, `Validating`, `SignedIn(login)`, and `Error(message)`. A successful fake bridge must save only after GitHub validation; a failed validation must not save; sign out clears the store.

- [ ] **Step 2: Implement Keystore-backed encryption**

`EncryptedBlobStore` generates alias `ramo.mobile.aes.v1` with `KeyProperties.KEY_ALGORITHM_AES`, 256-bit size, GCM block mode, and no padding. Store `version byte + 12-byte IV + ciphertext/tag` in app-private files. Never use `EncryptedSharedPreferences` and never log plaintext.

Expose:

```kotlin
interface TokenStore {
    fun read(): String?
    fun write(token: String)
    fun clear()
}
```

- [ ] **Step 3: Export session validation from Rust**

```rust
#[derive(uniffi::Record)]
pub struct MobileViewer { pub login: String }

#[derive(uniffi::Object)]
pub struct MobileSession { client: ramo_github::GithubClient }

#[uniffi::export]
impl MobileSession {
    #[uniffi::constructor]
    pub fn new(token: String) -> Result<std::sync::Arc<Self>, MobileError>;
    pub fn viewer(&self) -> Result<MobileViewer, MobileError>;
}
```

Map errors to sanitized user messages and stable kinds; do not expose HTTP bodies containing credentials.

`AuthViewModel` owns the generated `MobileSession` and calls its UniFFI `close()` method on sign out and `onCleared()` before dropping the reference.

- [ ] **Step 4: Build the token screen**

Use an obscured text field, `Paste token`, `Validate and continue`, a link that opens GitHub's fine-grained-token settings in the browser, and copy describing Pull requests write. Explain that team requests require selecting the organization as the token's resource owner; GitHub exposes no additional fine-grained permission for `GET /user/teams`. Do not request notification permission on this screen.

- [ ] **Step 5: Run unit and device crypto tests**

Run: `cd android && ./gradlew :app:testDebugUnitTest :app:connectedDebugAndroidTest`

Expected: ViewModel tests PASS; instrumented test proves stored bytes do not contain the plaintext token and clear removes the file.

- [ ] **Step 6: Commit authentication**

```bash
git add crates/ramo-mobile android/app/src
git commit -m "feat: secure github authentication on android"
```

### Task 4: Deliver the two-tab inbox

**Files:**
- Modify: `crates/ramo-mobile/src/lib.rs`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxModels.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxRepository.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxCache.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxViewModel.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/inbox/InboxScreen.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/inbox/InboxViewModelTest.kt`
- Create: `android/app/src/androidTest/kotlin/io/github/carlosarraes/ramo/inbox/InboxScreenTest.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/MainActivity.kt`

**Interfaces:**
- Consumes: Rust `list_inbox` and authenticated session.
- Produces: `Review requests` default tab, `Your PRs` tab, refresh, pagination, loading/empty/error states, and PR-open intent.

- [ ] **Step 1: Write failing state and UI tests**

Verify default tab is `ReviewRequests`, refresh replaces the first page, load-more appends without duplicate node IDs, switching tabs preserves each tab's list/cursor, cached rows appear while offline, and sign out clears the cache. Compose semantics must expose `Review requests`, `Your PRs`, repository/number, title, `+N`, `−N`, file count, and updated time.

- [ ] **Step 2: Export coarse inbox records from Rust**

Add UniFFI records mirroring core summaries and one method:

```rust
pub fn inbox(
    &self,
    kind: MobileInboxKind,
    after: Option<String>,
) -> Result<MobileInboxPage, MobileError>;
```

Perform all domain mapping in Rust so Kotlin does not parse GitHub JSON.

- [ ] **Step 3: Implement repository and ViewModel**

Use immutable `StateFlow<InboxUiState>` with independent `TabState` values. `refresh()` cancels only the selected tab's outstanding job, and `loadMore()` is a no-op when already loading or `hasNextPage` is false.

`InboxCache` stores both first-page tab states as one opaque Rust-encoded blob encrypted through `EncryptedBlobStore` alias `ramo.mobile.inbox.v1`. Add `encode_inbox_cache` and `decode_inbox_cache` bridge methods so Kotlin does not define a second serialization contract. Load the cache before the first network request, show `Offline · showing last refresh` when refresh fails without connectivity, and delete the cache on sign out. Do not cache diff/source bodies.

- [ ] **Step 4: Implement the approved compact Compose UI**

Use a small `ramo` wordmark, settings icon, two tabs, thin blue selected indicator, compact rows separated by one-pixel surface borders, colored additions/deletions, and no bottom navigation or oversized cards. Pull-to-refresh must not obscure the first row.

- [ ] **Step 5: Run inbox tests**

Run: `cd android && ./gradlew :app:testDebugUnitTest :app:connectedDebugAndroidTest`

Expected: all inbox tests PASS on JVM/emulator/device.

- [ ] **Step 6: Commit the inbox**

```bash
git add crates/ramo-mobile android/app/src
git commit -m "feat: add focused mobile pr inbox"
```

### Task 5: Add periodic review-request notifications and deep links

**Files:**
- Modify: `android/app/src/main/AndroidManifest.xml`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/notifications/NotificationCursorStore.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/notifications/ReviewNotificationWorker.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/notifications/NotificationScheduler.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/notifications/ReviewNotificationWorkerTest.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/MainActivity.kt`
- Modify: `crates/ramo-mobile/src/lib.rs`

**Interfaces:**
- Consumes: encrypted token, conditional cursor, Rust `review_notifications`.
- Produces: unique 15-minute WorkManager schedule, deduplicated Android notifications, and PR deep-link navigation.

- [ ] **Step 1: Write failing worker tests**

Test no token, 304, one new review request, duplicate notification ID, rate limit, revoked token, and retryable network failure. Assert only the new request posts a notification and the cursor updates only after successful processing.

- [ ] **Step 2: Export notification polling from Rust**

```rust
pub fn review_notifications(
    &self,
    etag: Option<String>,
    last_modified: Option<String>,
) -> Result<MobileNotificationPage, MobileError>;
```

- [ ] **Step 3: Implement WorkManager scheduling**

Use `PeriodicWorkRequestBuilder<ReviewNotificationWorker>(15, TimeUnit.MINUTES)`, `NetworkType.CONNECTED`, exponential backoff, and `enqueueUniquePeriodicWork("ramo-review-requests", ExistingPeriodicWorkPolicy.UPDATE, request)`.

On Android 13+, request `POST_NOTIFICATIONS` only after the signed-in inbox explains why. Disabling notifications does not disable manual refresh.

- [ ] **Step 4: Add notification channel and deep link**

Channel ID is `review_requests`, name `Review requests`, importance default. Each `PendingIntent` includes repository and PR number, uses immutable/update-current flags, and opens the PR reader route; until Plan 4 implements the reader, the app shows a stable loading screen with that identity.

- [ ] **Step 5: Run worker and APK tests**

Run: `cd android && ./gradlew :app:testDebugUnitTest :app:assembleDebug`

Expected: PASS.

- [ ] **Step 6: Install the first functional APK**

Run:

```bash
adb devices -l
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n io.github.carlosarraes.ramo/.MainActivity
```

Expected: device `192.168.15.7:5555` is online; Ramo opens to token entry or the two-tab inbox; no crash appears in `adb logcat -d -t 300` filtered for `AndroidRuntime`.

- [ ] **Step 7: Commit notifications**

```bash
git add crates/ramo-mobile android/app/src
git commit -m "feat: notify mobile review requests"
```
