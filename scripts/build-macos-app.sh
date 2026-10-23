#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
app="$root/target/macos/Button Designer.app"
products="$root/target/xcode/Release"
sign_identity=${CODE_SIGN_IDENTITY:--}
sign_options=(--force --sign "$sign_identity" --options runtime)
if [[ "$sign_identity" == "-" ]]; then
    sign_options+=(--timestamp=none)
else
    sign_options+=(--timestamp)
fi

cd "$root"
app_version=$(sed -nE 's/^version = "([^"]+)"$/\1/p' Cargo.toml | head -n 1)
if [[ -z "$app_version" ]]; then
    printf 'Could not read the package version from Cargo.toml.\n' >&2
    exit 1
fi
cargo build --locked --release --bin button-studio
xcodebuild \
    -project macos/ButtonStudioQuickLook.xcodeproj \
    -alltargets \
    -configuration Release \
    "SYMROOT=$root/target/xcode" \
    "MARKETING_VERSION=$app_version" \
    "CURRENT_PROJECT_VERSION=$app_version" \
    CODE_SIGNING_ALLOWED=NO \
    build

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/PlugIns"
cp target/release/button-studio "$app/Contents/MacOS/button-studio"
cp macos/AppInfo.plist "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $app_version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $app_version" "$app/Contents/Info.plist"
cp -R "$products/ButtonStudioThumbnail.appex" "$app/Contents/PlugIns/"
cp -R "$products/ButtonStudioPreview.appex" "$app/Contents/PlugIns/"

for extension in "$app"/Contents/PlugIns/*.appex; do
    /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $app_version" "$extension/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $app_version" "$extension/Contents/Info.plist"
    codesign \
        "${sign_options[@]}" \
        --entitlements macos/QuickLook/QuickLook.entitlements \
        "$extension"
done
codesign \
    "${sign_options[@]}" \
        --entitlements macos/App.entitlements \
        "$app"
codesign --verify --deep --strict "$app"

printf 'Built %s\n' "$app"
if [[ "$sign_identity" == "-" ]]; then
    printf 'The bundle is ad-hoc signed; use CODE_SIGN_IDENTITY for distributable Finder extensions.\n'
fi
