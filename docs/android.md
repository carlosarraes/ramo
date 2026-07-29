# Ramo for Android

Ramo's personal Android app is a focused GitHub review client for arm64 devices. It shows pull requests where you are a requested reviewer and your own open pull requests, opens each PR on a compact Review Map, renders one syntax-highlighted unified-diff file at a time, synchronizes Viewed state, keeps encrypted inline drafts, and publishes Comment, Approve, or Request changes reviews atomically.

## GitHub token

Create a fine-grained personal access token at <https://github.com/settings/personal-access-tokens/new> and grant selected repositories **Pull requests: Read and write**. Ramo validates the token before it stores anything.

Direct review requests work with a user-owned token. For team review requests, GitHub requires the token's resource owner to be that organization; the `GET /user/teams` endpoint needs no additional fine-grained permission but only returns teams for the selected resource-owner organization. An organization may also require an administrator to approve the token.

The token is AES-256-GCM encrypted with a non-exportable Android Keystore key. Sign out deletes the token, inbox cache, and all draft files. Full diffs and fetched source context are never persisted.

## Review flow

- `Review requests` is the default inbox; `Your PRs` is separate.
- Pull to refresh or use `Load more` for another page. Search matches repository, title, author, or PR number.
- A PR opens on its exact Review Map. Totals, classification, files, folds, and viewed state come from the already-fetched GitHub snapshot and work without the laptop.
- Authored groups begin expanded; test and generated groups begin collapsed. Tap a group to fold it, or Start/select a file to enter the unified one-file reviewer. Back returns to the same map selection and folds.
- Code scrolls horizontally; `current / total` opens the changed-file sheet; Previous and Next remain at the bottom.
- Tap a commentable code line to select it. Tap another compatible line to extend a contiguous range, then use `Comment` to open the composer. Enter inserts a newline; only `Save draft` finishes editing.
- Tap a collapsed unchanged-lines row to fetch and expand that source context. Expanded-only lines cannot receive GitHub comments.
- Existing GitHub conversations are read-only.
- Reaching the real end of a file marks it Viewed and offers an immediate Undo. The checkbox can also change Viewed state explicitly.
- `Finish` offers Comment, Approve, and Request changes, an optional overall comment, and a final confirmation. Self-authored PRs offer Comment only.
- If the PR head changes, publication is blocked and drafts remain encrypted until you explicitly repair or delete them.
- Account identity, code size, notification permission, and sign-out live under Settings rather than competing with the review queue.

## Private laptop analysis

AI summaries are optional enrichment. They run through `ramo-server` and Ollama on your laptop; the phone sends only repository, PR number, and expected head SHA. It never sends patches or source lines to the Review Map API and never connects to Ollama directly. The selected local model is recorded by `ramo server benchmark select`; on the current benchmark machine that is `qwen2.5-coder:7b`.

Both devices must be on the same tailnet and the server endpoint must use valid HTTPS under `.ts.net`. On the laptop:

```bash
ramo server setup
ramo server pair
```

Scan/open the printed `ramo://pair` link on the phone and confirm the hostname. The one-time code remains in memory only; the resulting client token is AES-256-GCM encrypted under a separate Android Keystore alias from the GitHub token. Settings shows the paired hostname and provides confirmed unpairing. Ramo first asks the laptop to revoke the client and always removes the local credential; if the laptop is asleep or unreachable, it explains that server-side revocation is still pending.

Laptop sleep, stopped Ollama, Tailscale loss, stale PR heads, timeouts, and incompatible versions produce a dismissible banner. The exact map and encrypted drafts remain usable; use Retry after connectivity returns. AI output is accepted only for exact changed paths and is discarded when its head SHA does not match the current snapshot.

## Notifications

After sign-in, Ramo offers an optional Android notification permission prompt. WorkManager checks the Review requests inbox approximately every 15 minutes while the device has network access. The first successful check establishes a baseline and does not notify for every existing PR; later unseen PR node IDs produce notifications. Android scheduling and battery policy can delay checks.

Ramo polls the review-request search rather than GitHub's notifications endpoint because GitHub does not allow fine-grained PATs on that endpoint.

## Build and install

Requirements are JDK 17, Rust 1.97, Android SDK 36, build-tools 36.0.0, NDK 28.2.13676358, and cargo-ndk 4.1.2. The bootstrap script installs the Android-side dependencies and writes the ignored `android/local.properties` file:

```bash
scripts/bootstrap-android.sh
cd android
./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug :app:assembleDebugAndroidTest
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Only `arm64-v8a` is produced in v1. GitLab, Bitbucket, replies/resolution of existing conversations, offline full-diff storage, and multi-user account switching are not included.

## Personal release signing

Release signing configuration lives in ignored `android/keystore.properties` and points to a keystore outside Git. With that file present:

```bash
cd android
./gradlew :app:assembleRelease
apksigner verify --verbose app/build/outputs/apk/release/app-release.apk
adb install -r app/build/outputs/apk/release/app-release.apk
```

## Mobile redesign acceptance

1. Run `./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug :app:assembleDebugAndroidTest` from `android/`.
2. Run `./gradlew :app:connectedDebugAndroidTest` with the unlocked ARM64 phone connected.
3. Install with `adb install -r app/build/outputs/apk/debug/app-debug.apk`.
4. Open Ramo and confirm the queue starts below the status bar, uses seamless rows, and shows changed-file counts.
5. Open an actual Mondrio PR and confirm the process remains alive, its exact Review Map is readable, and selecting a non-first file opens that file.
6. Open the file sheet from `current / total`, select another file, and use Previous/Next.
7. Select a line range, save a multiline draft, reach the file end, undo Viewed, and reopen the draft.
8. Open Finish, verify the exact draft count and verdict, cancel without publishing, and return to the cached queue.
