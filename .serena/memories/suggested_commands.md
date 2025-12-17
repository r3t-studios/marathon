# Suggested Commands for Aspen Development

## Build & Run Commands

### iOS Simulator (Primary Development Target)
```bash
# Build, deploy, and run on iOS Simulator (most common)
cargo xtask ios-run

# Build only (release mode)
cargo xtask ios-build

# Build in debug mode
cargo xtask ios-build --debug

# Deploy to specific device
cargo xtask ios-deploy --device "iPad Air (5th generation)"

# Run with debug mode and custom device
cargo xtask ios-run --debug --device "iPhone 15 Pro"

# Build and deploy to physical iPad
cargo xtask ios-device
```

### Desktop (macOS)
```bash
# Run on macOS desktop
cargo run --package app --features desktop

# Run in release mode
cargo run --package app --features desktop --release
```

## Testing
```bash
# Run all tests
cargo test

# Run tests for specific package
cargo test --package libmarathon
cargo test --package app

# Run integration tests
cargo test --test sync_integration

# Run with specific test
cargo test test_sync_between_two_nodes

# Run tests with logging output
RUST_LOG=debug cargo test -- --nocapture
```

## Code Quality

### Formatting
```bash
# Format all code (uses rustfmt.toml configuration)
cargo fmt

# Check formatting without modifying files
cargo fmt -- --check
```

### Linting
```bash
# Run clippy for all crates
cargo clippy --all-targets --all-features

# Run clippy with fixes
cargo clippy --fix --allow-dirty --allow-staged

# Strict clippy checks
cargo clippy -- -D warnings
```

### Building
```bash
# Build all crates
cargo build

# Build in release mode
cargo build --release

# Build specific package
cargo build --package libmarathon

# Build for iOS target
cargo build --target aarch64-apple-ios --release
cargo build --target aarch64-apple-ios-sim --release
```

## Cleaning
```bash
# Clean build artifacts
cargo clean

# Clean specific package
cargo clean --package xtask

# Clean and rebuild
cargo clean && cargo build
```

## Benchmarking
```bash
# Run benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench write_buffer
cargo bench --bench vector_clock
```

## Documentation
```bash
# Generate and open documentation
cargo doc --open

# Generate docs for all dependencies
cargo doc --open --document-private-items
```

## Dependency Management
```bash
# Update dependencies
cargo update

# Check for outdated dependencies
cargo outdated

# Show dependency tree
cargo tree

# Check specific dependency
cargo tree -p iroh
```

## iOS-Specific Commands

### Simulator Management
```bash
# List available simulators
xcrun simctl list devices available

# Boot a specific simulator
xcrun simctl boot "iPad Pro 12.9-inch M2"

# Open Simulator app
open -a Simulator

# View simulator logs
xcrun simctl spawn <device-uuid> log stream --predicate 'processImagePath contains "Aspen"'
```

### Device Management
```bash
# List connected devices
xcrun devicectl list devices

# View device logs
xcrun devicectl device stream log --device <device-id> --predicate 'process == "app"'
```

## Git Commands (macOS-specific notes)
```bash
# Standard git commands work on macOS
git status
git add .
git commit -m "message"
git push

# View recent commits
git log --oneline -10

# Check current branch
git branch
```

## System Commands (macOS)
```bash
# Find files (macOS has both find and mdfind)
find . -name "*.rs"
mdfind -name "rustfmt.toml"

# Search in files
grep -r "pattern" crates/
rg "pattern" crates/  # if ripgrep is installed

# List files
ls -la
ls -lh  # human-readable sizes

# Navigate
cd crates/app
pwd

# View file contents
cat Cargo.toml
head -20 src/main.rs
tail -50 Cargo.lock
```

## Common Workflows

### After Making Changes
```bash
# 1. Format code
cargo fmt

# 2. Run clippy
cargo clippy --all-targets

# 3. Run tests
cargo test

# 4. Test on simulator
cargo xtask ios-run
```

### Adding a New Feature
```bash
# 1. Create RFC if it's a major change
# edit docs/rfcs/NNNN-feature-name.md

# 2. Implement
# edit crates/.../src/...

# 3. Add tests
# edit crates/.../tests/...

# 4. Update documentation
cargo doc --open

# 5. Run full validation
cargo fmt && cargo clippy && cargo test && cargo xtask ios-run
```