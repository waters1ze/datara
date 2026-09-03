#!/usr/bin/env bash
# scripts/install_linux_icons.sh
# Installs Datara MIME type and official file icons on Linux (GNOME, KDE Plasma, XFCE)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Target directories: user-level if non-root, system-level if root
if [ "$(id -u)" -eq 0 ]; then
    MIME_DIR="/usr/share/mime/packages"
    ICON_BASE="/usr/share/icons/hicolor"
    APPS_DIR="/usr/share/applications"
else
    MIME_DIR="${HOME}/.local/share/mime/packages"
    ICON_BASE="${HOME}/.local/share/icons/hicolor"
    APPS_DIR="${HOME}/.local/share/applications"
fi

echo "Installing Datara MIME type and file icons..."
mkdir -p "${MIME_DIR}" "${APPS_DIR}"

# 1. Install MIME XML
cp "${REPO_ROOT}/assets/datara.xml" "${MIME_DIR}/datara.xml"

# 2. Install Scalable SVG Icon
mkdir -p "${ICON_BASE}/scalable/mimetypes" "${ICON_BASE}/scalable/apps"
cp "${REPO_ROOT}/assets/icon.svg" "${ICON_BASE}/scalable/mimetypes/text-x-datara.svg"
cp "${REPO_ROOT}/assets/icon.svg" "${ICON_BASE}/scalable/apps/datara.svg"

# 3. Install PNG Icons
for sz in 16 32 48 64 128 256 512; do
    mkdir -p "${ICON_BASE}/${sz}x${sz}/mimetypes" "${ICON_BASE}/${sz}x${sz}/apps"
    if command -v convert >/dev/null 2>&1; then
        convert "${REPO_ROOT}/assets/datara-logo.png" -resize "${sz}x${sz}" "${ICON_BASE}/${sz}x${sz}/mimetypes/text-x-datara.png"
        convert "${REPO_ROOT}/assets/datara-logo.png" -resize "${sz}x${sz}" "${ICON_BASE}/${sz}x${sz}/apps/datara.png"
    else
        # Fallback to high-res PNG
        cp "${REPO_ROOT}/assets/datara-logo.png" "${ICON_BASE}/${sz}x${sz}/mimetypes/text-x-datara.png"
        cp "${REPO_ROOT}/assets/datara-logo.png" "${ICON_BASE}/${sz}x${sz}/apps/datara.png"
    fi
done

# 4. Update MIME Database
if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$(dirname "${MIME_DIR}")" 2>/dev/null || true
fi

# 5. Update Icon Cache
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${ICON_BASE}" 2>/dev/null || true
fi

echo "✓ Datara file icon successfully installed on Linux! .dtr files will now display the official Datara icon."
