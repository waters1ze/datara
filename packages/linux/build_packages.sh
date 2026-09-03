#!/usr/bin/env bash
set -euo pipefail

# Build .deb and .rpm packages from release binaries
VERSION="0.1.0"
STAGE_DIR="/tmp/datara_pkg_stage"
OUTPUT_DIR="$(pwd)/dist"
mkdir -p "$OUTPUT_DIR"

echo "==> Preparing package staging directory..."
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/usr/bin"
mkdir -p "$STAGE_DIR/usr/lib/datara"

# Copy binaries
cp target/release/forgen target/release/datara target/release/dpm "$STAGE_DIR/usr/bin/"
chmod 755 "$STAGE_DIR/usr/bin/"*

# Copy stdlib and runtime
cp -r stdlib runtime "$STAGE_DIR/usr/lib/datara/"

# 1. Build .deb package
if command -v dpkg-deb >/dev/null 2>&1; then
    echo "==> Building Debian (.deb) package..."
    DEB_DIR="/tmp/datara_deb"
    rm -rf "$DEB_DIR"
    mkdir -p "$DEB_DIR"
    cp -r "$STAGE_DIR/"* "$DEB_DIR/"
    cp -r packages/linux/deb/DEBIAN "$DEB_DIR/"
    chmod 755 "$DEB_DIR/DEBIAN/postinst"
    dpkg-deb --build "$DEB_DIR" "$OUTPUT_DIR/datara_${VERSION}_amd64.deb"
    echo "==> Created: $OUTPUT_DIR/datara_${VERSION}_amd64.deb"
fi

# 2. Build .rpm package
if command -v rpmbuild >/dev/null 2>&1; then
    echo "==> Building RedHat (.rpm) package..."
    RPM_ROOT="/tmp/rpmbuild"
    rm -rf "$RPM_ROOT"
    mkdir -p "$RPM_ROOT"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    cp -r "$STAGE_DIR/"* "$RPM_ROOT/SOURCES/"
    cp packages/linux/rpm/datara.spec "$RPM_ROOT/SPECS/"
    rpmbuild --define "_topdir $RPM_ROOT" -bb "$RPM_ROOT/SPECS/datara.spec"
    cp "$RPM_ROOT"/RPMS/x86_64/*.rpm "$OUTPUT_DIR/"
    echo "==> Created RPM package in $OUTPUT_DIR"
fi

echo "==> Packaging complete!"