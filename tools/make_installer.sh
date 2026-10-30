#!/bin/bash
# Builds the universal (Apple Silicon + Intel) VST3 and wraps it in a macOS installer package that
# puts "ELABS RLX.vst3" in /Library/Audio/Plug-Ins/VST3. Output: dist/ELABS RLX <version>.pkg
#
# The package is unsigned: macOS asks to confirm it (right click > Open, or System Settings >
# Privacy & Security > Open Anyway). Signing and notarizing needs an Apple Developer ID:
#   SIGN_ID="Developer ID Installer: NAME (TEAMID)" tools/make_installer.sh
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
plugin="target/bundled/ELABS RLX.vst3"

[ "${SKIP_BUILD:-}" ] || cargo xtask bundle-universal elabs_lxr --release
lipo -info "$plugin/Contents/MacOS/ELABS RLX"
codesign --force --deep -s - "$plugin"

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
mkdir -p "$root/payload" dist
cp -R "$plugin" "$root/payload/"

pkgbuild --root "$root/payload" --install-location /Library/Audio/Plug-Ins/VST3 \
    --identifier com.elabs.rlx.vst3 --version "$version" "$root/vst3.pkg"

cat > "$root/distribution.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>ELABS RLX $version</title>
    <options customize="never" require-scripts="false" hostArchitectures="arm64,x86_64"/>
    <domains enable_localSystem="true"/>
    <choices-outline><line choice="vst3"/></choices-outline>
    <choice id="vst3" title="ELABS RLX VST3"><pkg-ref id="com.elabs.rlx.vst3"/></choice>
    <pkg-ref id="com.elabs.rlx.vst3" version="$version">vst3.pkg</pkg-ref>
</installer-gui-script>
EOF

out="dist/ELABS RLX $version.pkg"
productbuild --distribution "$root/distribution.xml" --package-path "$root" \
    ${SIGN_ID:+--sign "$SIGN_ID"} "$out"
echo "$out"
