#!/bin/bash
set -e

# iOS App Bundle Packaging Script
# Creates a proper .app bundle for iOS simulator deployment

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

APP_NAME="Aspen"
BUNDLE_ID="io.r3t.aspen"
TARGET="aarch64-apple-ios-sim"
BUILD_MODE="release"

BINARY_PATH="$PROJECT_ROOT/target/$TARGET/$BUILD_MODE/app"
APP_BUNDLE="$PROJECT_ROOT/target/$TARGET/$BUILD_MODE/$APP_NAME.app"

echo "📦 Packaging iOS app bundle..."
echo "Binary: $BINARY_PATH"
echo "Bundle: $APP_BUNDLE"

# Check if binary exists
if [ ! -f "$BINARY_PATH" ]; then
    echo "❌ Binary not found! Run build-simulator.sh first."
    exit 1
fi

# Remove old bundle if it exists
if [ -d "$APP_BUNDLE" ]; then
    echo "🗑️  Removing old bundle..."
    rm -rf "$APP_BUNDLE"
fi

# Create .app bundle structure
echo "📁 Creating bundle structure..."
mkdir -p "$APP_BUNDLE"

# Copy binary
echo "📋 Copying binary..."
cp "$BINARY_PATH" "$APP_BUNDLE/app"

# Copy Info.plist
echo "📋 Copying Info.plist..."
cp "$SCRIPT_DIR/Info.plist" "$APP_BUNDLE/Info.plist"

# Create PkgInfo
echo "📋 Creating PkgInfo..."
echo -n "APPL????" > "$APP_BUNDLE/PkgInfo"

# Set executable permissions
chmod +x "$APP_BUNDLE/app"

echo "✅ App bundle created successfully!"
echo "Bundle location: $APP_BUNDLE"
echo ""
echo "Bundle contents:"
ls -lh "$APP_BUNDLE"
