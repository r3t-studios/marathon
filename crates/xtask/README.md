# xtask - Build Automation for Aspen

This crate provides build automation tasks for the Aspen workspace using the [cargo-xtask pattern](https://github.com/matklad/cargo-xtask).

## Usage

All commands are run from the workspace root:

```bash
# Build for iOS Simulator (release mode)
cargo xtask ios-build

# Build for iOS Simulator (debug mode)
cargo xtask ios-build --debug

# Deploy to iOS Simulator
cargo xtask ios-deploy

# Deploy to a specific device
cargo xtask ios-deploy --device "iPad Air (5th generation)"

# Build, deploy, and stream logs (most common)
cargo xtask ios-run

# Build, deploy, and stream logs (debug mode)
cargo xtask ios-run --debug

# Build, deploy, and stream logs (specific device)
cargo xtask ios-run --device "iPhone 15 Pro"
```

## Available Commands

- **`ios-build`**: Builds the app for iOS Simulator (aarch64-apple-ios-sim)
  - `--debug`: Build in debug mode (default is release)

- **`ios-deploy`**: Deploys the app to iOS Simulator
  - `--device`: Device name (default: "iPad Pro 12.9-inch M2")

- **`ios-run`**: Builds, deploys, and streams logs (all-in-one)
  - `--debug`: Build in debug mode
  - `--device`: Device name (default: "iPad Pro 12.9-inch M2")

## How It Works

The xtask pattern creates a new binary target in your workspace that contains build automation code. This is better than shell scripts because:

1. **Cross-platform**: Pure Rust, works everywhere Rust works
2. **Type-safe**: Configuration and arguments are type-checked
3. **Maintainable**: Can use the same error handling and libraries as your main code
4. **Fast**: Compiled Rust is much faster than shell scripts
5. **Integrated**: Can share code with your workspace

## Adding New Tasks

To add a new task:

1. Add a new variant to the `Commands` enum in `src/main.rs`
2. Implement the corresponding function
3. Add the function call in the `match` statement in `main()`

Example:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands

    /// Run tests on iOS Simulator
    IosTest {
        /// Device name
        #[arg(long, default_value = "iPad Pro 12.9-inch M2")]
        device: String,
    },
}

fn ios_test(device_name: &str) -> Result<()> {
    // Implementation here
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        // ... existing matches
        Commands::IosTest { device } => ios_test(&device),
    }
}
```

## Dependencies

- `anyhow`: Error handling
- `clap`: Command-line argument parsing
