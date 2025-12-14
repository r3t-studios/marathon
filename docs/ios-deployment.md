# iOS Deployment Guide

This guide covers building and deploying Aspen (built on Marathon engine) to iOS devices and simulators.

## Prerequisites

### Required Tools

1. **Xcode Command Line Tools** (not full Xcode IDE)
   ```bash
   xcode-select --install
   ```

2. **Rust iOS targets**
   ```bash
   # For simulator (M1/M2/M3 Macs)
   rustup target add aarch64-apple-ios-sim

   # For device
   rustup target add aarch64-apple-ios
   ```

3. **iOS Simulator Runtime** (one-time download, ~8GB)
   ```bash
   xcodebuild -downloadPlatform iOS
   ```

### Verify Installation

```bash
# Check Swift compiler
swiftc --version

# Check iOS SDK
xcrun --sdk iphoneos --show-sdk-path

# List available simulators
xcrun simctl list devices
```

## Building for iOS Simulator

### Quick Start

Run all steps in one go:

```bash
# From project root
./scripts/ios/build-simulator.sh
./scripts/ios/package-app.sh
./scripts/ios/deploy-simulator.sh
```

### Step-by-Step

#### 1. Build the Binary

```bash
cargo build \
    --package app \
    --target aarch64-apple-ios-sim \
    --release
```

**Output:** `target/aarch64-apple-ios-sim/release/app`

#### 2. Package as .app Bundle

iOS apps must be in a specific `.app` bundle structure:

```
Aspen.app/
├── app           # The executable binary
├── Info.plist    # App metadata
└── PkgInfo       # Legacy file (APPL????)
```

Run the packaging script:

```bash
./scripts/ios/package-app.sh
```

**Output:** `target/aarch64-apple-ios-sim/release/Aspen.app/`

#### 3. Deploy to Simulator

```bash
./scripts/ios/deploy-simulator.sh
```

This will:
1. Find and boot the iPad Pro 13-inch simulator
2. Open Simulator.app
3. Uninstall any previous version
4. Install the new .app bundle
5. Launch the app with console output

**To use a different simulator:**

```bash
./scripts/ios/deploy-simulator.sh "iPad Air 13-inch (M2)"
```

## Building for Physical Device

### Prerequisites

1. **Apple Developer Account** (free account works for local testing)
2. **Device connected via USB**
3. **Code signing setup**

### Build Steps

```bash
# Build for device
cargo build \
    --package app \
    --target aarch64-apple-ios \
    --release

# Package (similar to simulator)
# TODO: Create device packaging script with code signing
```

### Code Signing

iOS requires apps to be signed before running on device:

```bash
# Create signing identity (first time only)
security find-identity -v -p codesigning

# Sign the app bundle
codesign --force --sign "Apple Development: your@email.com" \
    target/aarch64-apple-ios/release/Aspen.app
```

### Deploy to Device

```bash
# Using devicectl (modern, requires macOS 13+)
xcrun devicectl device install app \
    --device <UUID> \
    target/aarch64-apple-ios/release/Aspen.app

# Launch
xcrun devicectl device process launch \
    --device <UUID> \
    io.r3t.aspen
```

## Project Structure

```
lonni/
├── crates/
│   ├── app/                    # Aspen application
│   └── libmarathon/            # Marathon engine
│       ├── src/platform/
│       │   ├── ios/
│       │   │   ├── executor.rs         # iOS winit executor
│       │   │   ├── pencil_bridge.rs    # Rust FFI
│       │   │   ├── PencilBridge.h      # C header
│       │   │   └── PencilCapture.swift # Swift capture
│       │   └── ...
│       └── build.rs            # Swift compilation
├── scripts/
│   └── ios/
│       ├── Info.plist          # App metadata template
│       ├── build-simulator.sh  # Build binary
│       ├── package-app.sh      # Create .app bundle
│       └── deploy-simulator.sh # Install & launch
└── docs/
    └── ios-deployment.md       # This file
```

## App Bundle Configuration

### Info.plist

Key fields in `scripts/ios/Info.plist`:

```xml
<key>CFBundleIdentifier</key>
<string>io.r3t.aspen</string>

<key>CFBundleName</key>
<string>Aspen</string>

<key>MinimumOSVersion</key>
<string>14.0</string>
```

To customize, edit `scripts/ios/Info.plist` before packaging.

## Apple Pencil Support

Aspen includes native Apple Pencil support via the Marathon engine:

- **Pressure sensitivity** (0.0 - 1.0)
- **Tilt angles** (altitude & azimuth)
- **Touch phases** (began/moved/ended/cancelled)
- **Timestamps** for precision input

The Swift → Rust bridge is compiled automatically during `cargo build` for iOS targets.

## Troubleshooting

### "runtime profile not found"

The iOS runtime isn't installed:

```bash
xcodebuild -downloadPlatform iOS
```

### "No signing identity found"

For simulator, signing is not required. For device:

```bash
# Check signing identities
security find-identity -v -p codesigning

# If none exist, create one in Xcode or via command line
```

### "Could not launch app"

Check simulator logs:

```bash
xcrun simctl spawn booted log stream --predicate 'processImagePath contains "Aspen"'
```

### Swift compilation fails

Ensure the iOS SDK is installed:

```bash
xcrun --sdk iphoneos --show-sdk-path
```

Should output a path like:
```
/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS26.2.sdk
```

## Performance Notes

### Simulator vs Device

- **Simulator**: Runs on your Mac's CPU (fast for development)
- **Device**: Runs on iPad's chip (true performance testing)

### Build Times

- **Simulator build**: ~30s (incremental), ~5min (clean)
- **Device build**: Similar, but includes Swift compilation

### App Size

- **Debug build**: ~200MB (includes debug symbols)
- **Release build**: ~50MB (optimized, stripped)

## Next Steps

- [ ] Add device deployment script with code signing
- [ ] Create GitHub Actions workflow for CI builds
- [ ] Document TestFlight deployment
- [ ] Add crash reporting integration

## References

- [Apple Developer Documentation](https://developer.apple.com/documentation/)
- [Rust iOS Guide](https://github.com/rust-mobile/rust-ios-android)
- [Winit iOS Platform](https://docs.rs/winit/latest/winit/platform/ios/)
