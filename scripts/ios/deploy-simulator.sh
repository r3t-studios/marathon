#!/bin/bash
set -e

# iOS Simulator Deploy Script
# Boots simulator, installs app, and launches it

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

APP_NAME="Aspen"
BUNDLE_ID="io.r3t.aspen"
TARGET="aarch64-apple-ios-sim"
BUILD_MODE="release"

APP_BUNDLE="$PROJECT_ROOT/target/$TARGET/$BUILD_MODE/$APP_NAME.app"

# Default to iPad Pro 12.9-inch M2 (matches user's physical device)
DEVICE_NAME="${1:-iPad Pro 12.9-inch M2}"

echo "📱 Deploying to iOS Simulator..."
echo "Device: $DEVICE_NAME"
echo "Bundle: $APP_BUNDLE"

# Check if bundle exists
if [ ! -d "$APP_BUNDLE" ]; then
    echo "❌ App bundle not found! Run package-app.sh first."
    exit 1
fi

# Get device UUID
echo "🔍 Finding device UUID..."
DEVICE_UUID=$(xcrun simctl list devices | grep "$DEVICE_NAME" | grep -v "unavailable" | head -1 | sed -E 's/.*\(([0-9A-F-]+)\).*/\1/')

if [ -z "$DEVICE_UUID" ]; then
    echo "❌ Device '$DEVICE_NAME' not found or unavailable."
    echo ""
    echo "Available devices:"
    xcrun simctl list devices | grep -i ipad | grep -v unavailable
    exit 1
fi

echo "✅ Found device: $DEVICE_UUID"

# Boot device if not already booted
echo "🚀 Booting simulator..."
xcrun simctl boot "$DEVICE_UUID" 2>/dev/null || true

# Wait a moment for boot
sleep 2

# Open Simulator.app
echo "📱 Opening Simulator.app..."
open -a Simulator

# Uninstall old version if it exists
echo "🗑️  Uninstalling old version..."
xcrun simctl uninstall "$DEVICE_UUID" "$BUNDLE_ID" 2>/dev/null || true

# Install the app
echo "📲 Installing app..."
xcrun simctl install "$DEVICE_UUID" "$APP_BUNDLE"

# Launch the app
echo "🚀 Launching app..."
xcrun simctl launch --console "$DEVICE_UUID" "$BUNDLE_ID"

echo "✅ App deployed and launched!"
echo ""
echo "To view logs:"
echo "  xcrun simctl spawn $DEVICE_UUID log stream --predicate 'processImagePath contains \"$APP_NAME\"'"
