#!/bin/bash
set -e

# iOS Simulator Build Script
# Builds the Marathon app for iOS simulator (aarch64-apple-ios-sim)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "🔨 Building for iOS Simulator..."
echo "Project root: $PROJECT_ROOT"

# Build for simulator target
cd "$PROJECT_ROOT"
cargo build \
    --package app \
    --target aarch64-apple-ios-sim \
    --release

echo "✅ Build complete!"
echo "Binary location: $PROJECT_ROOT/target/aarch64-apple-ios-sim/release/app"
