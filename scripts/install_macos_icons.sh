#!/usr/bin/env bash
# scripts/install_macos_icons.sh
# Installs Datara file icon and associations on macOS Finder

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ICNS_PATH="${REPO_ROOT}/assets/datara.icns"

echo "Configuring Datara file icons for macOS Finder..."

# If duti is available, register UTI
if command -v duti >/dev/null 2>&1; then
    duti -s com.microsoft.VSCode .dtr all 2>/dev/null || true
fi

# Create a lightweight Datara.app stub in /Applications or ~/Applications to register Finder icon
APP_DIR="${HOME}/Applications/Datara.app"
mkdir -p "${APP_DIR}/Contents/Resources" "${APP_DIR}/Contents/MacOS"

cp "${ICNS_PATH}" "${APP_DIR}/Contents/Resources/datara.icns"

cat > "${APP_DIR}/Contents/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>DataraLauncher</string>
    <key>CFBundleIconFile</key>
    <string>datara.icns</string>
    <key>CFBundleIdentifier</key>
    <string>com.datara.launcher</string>
    <key>CFBundleName</key>
    <string>Datara</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>dtr</string>
                <string>datara</string>
            </array>
            <key>CFBundleTypeIconFile</key>
            <string>datara.icns</string>
            <key>CFBundleTypeName</key>
            <string>Datara Source File</string>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
            <key>LSHandlerRank</key>
            <string>Owner</string>
        </dict>
    </array>
</dict>
</plist>
EOF

cat > "${APP_DIR}/Contents/MacOS/DataraLauncher" << 'EOF'
#!/bin/bash
if [ -n "$1" ]; then
    forgen run "$1"
else
    forgen repl
fi
EOF
chmod +x "${APP_DIR}/Contents/MacOS/DataraLauncher"

# Rebuild LaunchServices database
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "${APP_DIR}" 2>/dev/null || true

echo "✓ Datara file icon successfully registered for macOS Finder!"
