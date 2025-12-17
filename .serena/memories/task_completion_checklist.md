# Task Completion Checklist

When completing a task in Aspen, follow these steps to ensure code quality and consistency.

## Pre-Commit Checklist

### 1. Code Formatting
```bash
cargo fmt
```
- Formats all code according to `rustfmt.toml`
- Must pass before committing
- Check with: `cargo fmt -- --check`

### 2. Linting
```bash
cargo clippy --all-targets --all-features
```
- Checks for common mistakes and anti-patterns
- Address all warnings
- For strict mode: `cargo clippy -- -D warnings`

### 3. Type Checking & Compilation
```bash
cargo check
cargo build
```
- Ensure code compiles without errors
- Check both debug and release if performance-critical:
  ```bash
  cargo build --release
  ```

### 4. Testing
```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run integration tests
cargo test --test sync_integration
```
- All existing tests must pass
- Add new tests for new functionality
- Integration tests for cross-component features

### 5. Platform-Specific Testing

#### iOS Simulator
```bash
cargo xtask ios-run
```
- Test on iOS Simulator (default: iPad Pro 12.9-inch M2)
- Verify Apple Pencil interactions if applicable
- Check logging output for errors

#### Physical Device (if iOS changes)
```bash
cargo xtask ios-device
```
- Test on actual iPad if Apple Pencil features are involved
- Verify Developer Mode is enabled

#### macOS Desktop
```bash
cargo run --package app --features desktop
```
- Test desktop functionality
- Verify window handling and input

### 6. Documentation
```bash
cargo doc --open
```
- Add doc comments to public APIs
- Update module-level documentation if structure changed
- Verify generated docs render correctly
- Update RFCs if architectural changes were made

## Specific Checks by Change Type

### For New Features
- [ ] Write RFC if architectural change (see `docs/rfcs/README.md`)
- [ ] Add public API documentation
- [ ] Add examples in doc comments
- [ ] Write integration tests
- [ ] Test on both macOS and iOS if cross-platform
- [ ] Update relevant memory files if workflow changes

### For Bug Fixes
- [ ] Add regression test
- [ ] Document the bug in commit message
- [ ] Verify fix on affected platform(s)
- [ ] Check for similar bugs in related code

### For Performance Changes
- [ ] Run benchmarks before and after
  ```bash
  cargo bench
  ```
- [ ] Document performance impact in commit message
- [ ] Test on debug and release builds

### For Refactoring
- [ ] Ensure all tests still pass
- [ ] Verify no behavioral changes
- [ ] Update related documentation
- [ ] Check that clippy warnings didn't increase

### For Dependency Updates
- [ ] Update `Cargo.toml` (workspace or specific crate)
- [ ] Run `cargo update`
- [ ] Check for breaking changes in changelog
- [ ] Re-run full test suite
- [ ] Test on both platforms

## Before Pushing

### Final Validation
```bash
# One-liner for comprehensive check
cargo fmt && cargo clippy --all-targets && cargo test && cargo xtask ios-run
```

### Git Checks
- [ ] Review `git diff` for unintended changes
- [ ] Ensure sensitive data isn't included
- [ ] Write clear commit message (see code_style_conventions.md)
- [ ] Verify correct branch

### Issue Tracking
- [ ] Update issue status (use GitHub issue templates)
- [ ] Link commits to issues in commit message
- [ ] Update project board if using one

## Platform-Specific Considerations

### iOS Changes
- [ ] Test on iOS Simulator
- [ ] Verify Info.plist changes if app metadata changed
- [ ] Check Entitlements.plist if permissions changed
- [ ] Test with Apple Pencil if input handling changed
- [ ] Verify app signing (bundle ID: `G872CZV7WG.aspen`)

### Networking Changes
- [ ] Test P2P connectivity on local network
- [ ] Verify gossip propagation with multiple peers
- [ ] Check CRDT merge behavior with concurrent edits
- [ ] Test with network interruptions

### Persistence Changes
- [ ] Test database migrations if schema changed
- [ ] Verify data integrity across app restarts
- [ ] Check SQLite WAL mode behavior
- [ ] Test with large datasets

### UI Changes
- [ ] Test with debug UI enabled
- [ ] Verify on different screen sizes (iPad, desktop)
- [ ] Check touch and mouse input paths
- [ ] Test accessibility if UI changed

## Common Issues to Watch For

### Compilation
- Missing feature flags for conditional compilation
- Platform-specific code not properly gated with `#[cfg(...)]`
- Incorrect use of async/await in synchronous contexts

### Runtime
- Panics in production code (should return `Result` instead)
- Deadlocks with locks (use `parking_lot` correctly)
- Memory leaks with Arc/Rc cycles
- Thread spawning without proper cleanup

### iOS-Specific
- Using `println!` instead of `tracing` (doesn't work on iOS)
- Missing `tracing-oslog` initialization
- Incorrect bundle ID or entitlements
- Not testing on actual device for Pencil features

## When Task is Complete

1. **Run final validation**:
   ```bash
   cargo fmt && cargo clippy && cargo test && cargo xtask ios-run
   ```

2. **Commit with good message**:
   ```bash
   git add .
   git commit -m "Clear, descriptive message"
   ```

3. **Push to remote**:
   ```bash
   git push origin <branch-name>
   ```

4. **Create pull request** (if working in feature branch):
   - Reference related issues
   - Describe changes and rationale
   - Note any breaking changes
   - Request review if needed

5. **Update documentation**:
   - Update RFCs if architectural change
   - Update memory files if workflow changed
   - Update README if user-facing change