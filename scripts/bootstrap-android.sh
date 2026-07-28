#!/usr/bin/env bash
set -euo pipefail

tools_version=15859902
tools_sha=4e4c464f145a7512b57d088ac6c278c03c9eea610886b35a5e0804e74eedf583
sdk_root="${ANDROID_SDK_ROOT:-${XDG_DATA_HOME:-$HOME/.local/share}/android-sdk}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdkmanager="$sdk_root/cmdline-tools/latest/bin/sdkmanager"

if [[ ! -x "$sdkmanager" ]]; then
  work_dir="$(mktemp -d)"
  trap 'rm -rf "$work_dir"' EXIT
  archive="$work_dir/command-line-tools.zip"
  curl --fail --location --show-error \
    "https://dl.google.com/android/repository/commandlinetools-linux-${tools_version}_latest.zip" \
    --output "$archive"
  printf '%s  %s\n' "$tools_sha" "$archive" | sha256sum --check --status
  unzip -q "$archive" -d "$work_dir/unpacked"
  mkdir -p "$sdk_root/cmdline-tools"
  rm -rf "$sdk_root/cmdline-tools/latest"
  mv "$work_dir/unpacked/cmdline-tools" "$sdk_root/cmdline-tools/latest"
fi

yes | "$sdkmanager" --sdk_root="$sdk_root" --licenses >/dev/null || true
"$sdkmanager" --sdk_root="$sdk_root" \
  "platforms;android-36" \
  "build-tools;36.0.0" \
  "ndk;28.2.13676358" \
  "platform-tools"

rustup toolchain install 1.97.0 --profile minimal --component clippy,rustfmt
rustup target add --toolchain 1.97.0 aarch64-linux-android
if ! cargo ndk --version 2>/dev/null | grep -Fq 'cargo-ndk 4.1.2'; then
  cargo install cargo-ndk --version 4.1.2 --locked
fi

escaped_sdk="${sdk_root//\\/\\\\}"
escaped_sdk="${escaped_sdk// /\\ }"
printf 'sdk.dir=%s\n' "$escaped_sdk" > "$repo_root/android/local.properties"

if [[ ! -f "$repo_root/android/gradle/wrapper/gradle-wrapper.jar" ]]; then
  work_dir="${work_dir:-$(mktemp -d)}"
  trap 'rm -rf "$work_dir"' EXIT
  gradle_archive="$work_dir/gradle-9.4.1-bin.zip"
  curl --fail --location --show-error \
    'https://services.gradle.org/distributions/gradle-9.4.1-bin.zip' \
    --output "$gradle_archive"
  unzip -q "$gradle_archive" -d "$work_dir/gradle"
  "$work_dir/gradle/gradle-9.4.1/bin/gradle" \
    --no-daemon --project-dir "$repo_root/android" wrapper --gradle-version 9.4.1
fi

printf 'Android SDK ready at %s\n' "$sdk_root"
