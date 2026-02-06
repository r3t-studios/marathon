# Contributing to Marathon

Thank you for your interest in contributing to Marathon! We're excited to work with you.

This document provides guidelines for contributing to the project. Following these guidelines helps maintain code quality and makes the review process smoother for everyone.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Environment Setup](#development-environment-setup)
- [How to Contribute](#how-to-contribute)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Features](#suggesting-features)
- [AI Usage Policy](#ai-usage-policy)
- [Questions?](#questions)

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code. Please report unacceptable behavior to the project maintainers.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally
3. **Set up your development environment** (see below)
4. **Create a branch** for your changes
5. **Make your changes** with clear commit messages
6. **Test your changes** thoroughly
7. **Submit a pull request**

## Development Environment Setup

### Prerequisites

- **Rust** 2024 edition or later (install via [rustup](https://rustup.rs/))
- **macOS** (for macOS desktop and iOS development)
- **Xcode** and iOS simulator (for iOS development)
- **Linux** (for Linux desktop development)
- **Windows** (for Windows desktop development)
- **Git** for version control

### Initial Setup

```bash
# Clone your fork
git clone https://github.com/user/marathon.git
cd marathon

# Add upstream remote
git remote add upstream https://github.com/r3t-studios/marathon.git

# Build the project
cargo build

# Run tests
cargo test

# Run the desktop demo
cargo run --package app
```

### iOS Development Setup

For iOS development, see our detailed [iOS Deployment Guide](docs/ios-deployment.md).

```bash
# Build for iOS simulator
cargo xtask ios-build

# Run on simulator
cargo xtask ios-run
```

### Useful Commands

```bash
# Check code without building
cargo check

# Run clippy for linting
cargo clippy

# Format code
cargo fmt

# Run tests with output
cargo nextest run -- --nocapture

# Build documentation
cargo doc --open
```

## How to Contribute

### Types of Contributions

We welcome many types of contributions:

- **Bug fixes** - Fix issues and improve stability
- **Features** - Implement new functionality (discuss first in an issue)
- **Documentation** - Improve or add documentation
- **Examples** - Create new examples or demos
- **Tests** - Add test coverage
- **Performance** - Optimize existing code
- **Refactoring** - Improve code quality

### Before You Start

For **bug fixes and small improvements**, feel free to open a PR directly.

For **new features or significant changes**:
1. **Open an issue first** to discuss the proposal
2. Wait for maintainer feedback before investing significant time
3. Reference the issue in your PR

This helps ensure your work aligns with project direction and avoids duplicate effort.

## Coding Standards

### Rust Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Follow the [Rust Style Guide](https://microsoft.github.io/rust-guidelines/guidelines/index.html)
- Use `cargo +nightly fmt` to format code (run before committing)
- Address all `cargo clippy` warnings
- Use meaningful variable and function names
- Add doc comments (`///`) for public APIs

### Code Organization

- Keep modules focused and cohesive
- Prefer composition over inheritance
- Use Rust's type system to enforce invariants
- Avoid unnecessary `unsafe` code

### Documentation

- Add doc comments for all public types, traits, and functions
- Include examples in doc comments when helpful
- Update relevant documentation in `/docs` when making architectural changes
- Keep README.md in sync with current capabilities

### Commit Messages

Write clear, descriptive conventional commit messages:

```
Short summary (50 chars or less)

More detailed explanation if needed. Wrap at 72 characters.

- Bullet points are fine
- Use present tense ("Add feature" not "Added feature")
- Reference issues and PRs with #123
```

Good examples:
```
Add cursor synchronization to networking layer

Implement entity selection system for iOS

Fix panic in SQLite persistence during shutdown (#42)
```

## Testing

### Running Tests

```bash
# Run all tests
cargo nextest run

# Run tests for specific crate
cargo nextest run --package libmarathon

# Run specific test
cargo nextest run test_vector_clock_merge

# Run tests with output
cargo nextest run -- --nocapture
```

### Writing Tests

- Add unit tests in the same file as the code (in a `mod tests` block)
- Add integration tests in `tests/` directory
- Test edge cases and error conditions
- Keep tests focused and readable
- Use descriptive test names: `test_vector_clock_handles_concurrent_updates`

### Test Coverage

We aim for good test coverage, especially for:
- CRDT operations and synchronization logic
- Persistence layer operations
- Network protocol handling
- Error conditions and edge cases

You don't need 100% coverage, but core logic should be well-tested.

## Pull Request Process

### Before Submitting

1. **Update your branch** with latest upstream changes
   ```bash
   git fetch upstream
   git rebase upstream/mainline
   ```

2. **Run the test suite** and ensure all tests pass
   ```bash
   cargo test
   ```

3. **Run clippy** and fix any warnings
   ```bash
   cargo clippy
   ```

4. **Format your code**
   ```bash
   cargo fmt
   ```

5. **Update documentation** if you changed APIs or behavior

### Submitting Your PR

1. **Push to your fork**
   ```bash
   git push origin your-branch-name
   ```

2. **Open a pull request** on GitHub

3. **Fill out the PR template** with:
   - Clear description of what changed and why
   - Link to related issues
   - Testing performed
   - Screenshots/videos for UI changes

4. **Request review** from maintainers

### During Review

- Be responsive to feedback
- Make requested changes promptly
- Push updates to the same branch (they'll appear in the PR)
- Use "fixup" commits or force-push after addressing review comments
- Be patient - maintainers are volunteers with limited time

### After Approval

- Maintainers will merge your PR
- You can delete your branch after merging
- Celebrate! 🎉 You're now a Marathon contributor!

## Reporting Bugs

### Before Reporting

1. **Check existing issues** to avoid duplicates
2. **Verify it's a bug** and not expected behavior
3. **Test on the latest version** from mainline branch

### Bug Report Template

When opening a bug report, please include:

- **Description** - What went wrong?
- **Expected behavior** - What should have happened?
- **Actual behavior** - What actually happened?
- **Steps to reproduce** - Minimal steps to reproduce the issue
- **Environment**:
  - OS version (macOS version, iOS version)
  - Rust version (`rustc --version`)
  - Marathon version or commit hash
- **Logs/Stack traces** - Error messages or relevant log output
- **Screenshots/Videos** - If applicable

### Security Issues

**Do not report security vulnerabilities in public issues.**

Please see our [Security Policy](SECURITY.md) for how to report security issues privately.

## Suggesting Features

We welcome feature suggestions! Here's how to propose them effectively:

### Before Suggesting

1. **Check existing issues and discussions** for similar ideas
2. **Consider if it aligns** with Marathon's goals (multiplayer game engine framework)
3. **Think about the scope** - is this a core feature or better as a plugin/extension?

### Feature Request Template

When suggesting a feature, please include:

- **Problem statement** - What problem does this solve?
- **Proposed solution** - How would this feature work?
- **Alternatives considered** - What other approaches did you think about?
- **Use cases** - Real-world scenarios where this helps
- **Implementation ideas** - Technical approach (if you have thoughts)

### Feature Discussion

- Maintainers will label feature requests as `enhancement`
- We'll discuss feasibility, scope, and priority
- Features that align with the roadmap are more likely to be accepted
- You're welcome to implement features you propose (with approval)

## AI Usage Policy

Marathon has specific guidelines around AI and ML tool usage. Please read our [AI Usage Policy](AI_POLICY.md) before contributing.

**Key points:**
- AI tools (Copilot, ChatGPT, etc.) are allowed for productivity
- You must understand and be accountable for all code you submit
- Humans make all architectural decisions, not AI
- When in doubt, ask yourself: "Can I maintain and debug this?"

## Questions?

- **General questions** - Open a [Discussion](https://github.com/yourusername/marathon/discussions)
- **Bug reports** - Open an [Issue](https://github.com/yourusername/marathon/issues)
- **Real-time chat** - [Discord/Slack link if you have one]
- **Email** - [maintainer email if appropriate]

## Recognition

All contributors will be recognized in our release notes and can be listed in AUTHORS file (coming soon).

---

Thank you for contributing to Marathon! Your effort helps make collaborative software better for everyone.
