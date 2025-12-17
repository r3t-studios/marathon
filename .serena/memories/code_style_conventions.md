# Code Style & Conventions

## Rust Style Configuration
The project uses **rustfmt** with a custom configuration (`rustfmt.toml`):

### Key Formatting Rules
- **Edition**: 2021
- **Braces**: `PreferSameLine` for structs/enums, `AlwaysSameLine` for control flow
- **Function Layout**: `Tall` (each parameter on its own line for long signatures)
- **Single-line Functions**: Disabled (`fn_single_line = false`)
- **Imports**:
  - Grouping: `StdExternalCrate` (std, external, then local)
  - Layout: `Vertical` (one import per line)
  - Granularity: `Crate` level
  - Reorder: Enabled
- **Comments**:
  - Width: 80 characters
  - Wrapping: Enabled
  - Format code in doc comments: Enabled
- **Doc Attributes**: Normalized (`normalize_doc_attributes = true`)
- **Impl Items**: Reordered (`reorder_impl_items = true`)
- **Match Arms**: Leading pipes always shown
- **Hex Literals**: Lowercase

### Applying Formatting
```bash
# Format all code
cargo fmt

# Check without modifying
cargo fmt -- --check
```

## Naming Conventions

### Rust Standard Conventions
- **Types** (structs, enums, traits): `PascalCase`
  - Example: `EngineBridge`, `PersistenceConfig`, `SessionId`
- **Functions & Methods**: `snake_case`
  - Example: `run_executor()`, `get_database_path()`
- **Constants**: `SCREAMING_SNAKE_CASE`
  - Example: `APP_NAME`, `DEFAULT_BUFFER_SIZE`
- **Variables**: `snake_case`
  - Example: `engine_bridge`, `db_path_str`
- **Modules**: `snake_case`
  - Example: `debug_ui`, `engine_bridge`
- **Crates**: `kebab-case` in Cargo.toml, `snake_case` in code
  - Example: `sync-macros` → `sync_macros`

### Project-Specific Patterns
- **Platform modules**: `platform/desktop/`, `platform/ios/`
- **Plugin naming**: Suffix with `Plugin` (e.g., `EngineBridgePlugin`, `CameraPlugin`)
- **Resource naming**: Prefix with purpose (e.g., `PersistenceConfig`, `SessionManager`)
- **System naming**: Suffix with `_system` for Bevy systems
- **Bridge pattern**: Use `Bridge` suffix for inter-component communication (e.g., `EngineBridge`)

## Code Organization

### Module Structure
```rust
// Public API first
pub mod engine;
pub mod networking;
pub mod persistence;

// Internal modules
mod debug_ui;
mod platform;

// Re-exports for convenience
pub use engine::{EngineCore, EngineBridge};
```

### Import Organization
```rust
// Standard library
use std::sync::Arc;
use std::thread;

// External crates (grouped by crate)
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

// Internal crates
use libmarathon::engine::EngineCore;
use libmarathon::platform;

// Local modules
use crate::camera::*;
use crate::debug_ui::DebugUiPlugin;
```

## Documentation

### Doc Comments
- Use `///` for public items
- Use `//!` for module-level documentation
- Include examples where helpful
- Document panics, errors, and safety considerations

```rust
/// Creates a new engine bridge for communication between Bevy and EngineCore.
///
/// # Returns
///
/// A tuple of `(EngineBridge, EngineHandle)` where the bridge goes to Bevy
/// and the handle goes to EngineCore.
///
/// # Examples
///
/// ```no_run
/// let (bridge, handle) = EngineBridge::new();
/// app.insert_resource(bridge);
/// // spawn EngineCore with handle
/// ```
pub fn new() -> (EngineBridge, EngineHandle) {
    // ...
}
```

### Code Comments
- Keep line comments at 80 characters or less
- Explain *why*, not *what* (code should be self-documenting for the "what")
- Use `// TODO:` for temporary code that needs improvement
- Use `// SAFETY:` before unsafe blocks to explain invariants

## Error Handling

### Library Code (libmarathon)
- Use `thiserror` for custom error types
- Return `Result<T, Error>` from fallible functions
- Provide context with error chains

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("failed to connect to peer: {0}")]
    ConnectionFailed(String),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}
```

### Application Code (app)
- Use `anyhow::Result` for application-level error handling
- Add context with `.context()` or `.with_context()`

```rust
use anyhow::{Context, Result};

fn load_config() -> Result<Config> {
    let path = get_config_path()
        .context("failed to determine config path")?;
    
    std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config from {:?}", path))?
        // ...
}
```

## Async/Await Style

### Tokio Runtime Usage
- Spawn blocking tasks in background threads
- Use `tokio::spawn` for async tasks
- Prefer `async fn` over `impl Future`

```rust
// Good: Clear async function
async fn process_events(&mut self) -> Result<()> {
    // ...
}

// Background task spawning
std::thread::spawn(move || {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        core.run().await;
    });
});
```

## Testing Conventions

### Test Organization
- Unit tests: In same file as code (`#[cfg(test)] mod tests`)
- Integration tests: In `tests/` directory
- Benchmarks: In `benches/` directory

### Test Naming
- Use descriptive names: `test_sync_between_two_nodes`
- Use `should_` prefix for behavior tests: `should_reject_invalid_input`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_bridge_creation() {
        let (bridge, handle) = EngineBridge::new();
        // ...
    }

    #[tokio::test]
    async fn should_sync_state_across_peers() {
        // ...
    }
}
```

## Platform-Specific Code

### Feature Gates
```rust
// iOS-specific code
#[cfg(target_os = "ios")]
use tracing_oslog::OsLogger;

// Desktop-specific code
#[cfg(not(target_os = "ios"))]
use tracing_subscriber::fmt;
```

### Platform Modules
- Keep platform-specific code in `platform/` modules
- Provide platform-agnostic interfaces when possible
- Use feature flags: `desktop`, `ios`, `headless`

## Logging

### Use Structured Logging
```rust
use tracing::{debug, info, warn, error};

// Good: Structured with context
info!(path = %db_path, "opening database");
debug!(count = peers.len(), "connected to peers");

// Avoid: Plain string
info!("Database opened at {}", db_path);
```

### Log Levels
- `error!`: System failures requiring immediate attention
- `warn!`: Unexpected conditions that are handled
- `info!`: Important state changes and milestones
- `debug!`: Detailed diagnostic information
- `trace!`: Very verbose, rarely needed

## RFCs and Design Documentation

### When to Write an RFC
- Architectural decisions affecting multiple parts
- Choosing between significantly different approaches
- Introducing new protocols or APIs
- Making breaking changes

### RFC Structure (see `docs/rfcs/README.md`)
- Narrative-first explanation
- Trade-offs and alternatives
- API examples (not full implementations)
- Open questions
- Success criteria

## Git Commit Messages

### Format
```
Brief summary (50 chars or less)

More detailed explanation if needed. Wrap at 72 characters.

- Use bullet points for multiple changes
- Reference issue numbers: #123

Explains trade-offs, alternatives considered, and why this approach
was chosen.
```

### Examples
```
Add CRDT synchronization over iroh-gossip

Implements the protocol described in RFC 0001. Uses vector clocks
for causal ordering and merkle trees for efficient reconciliation.

- Add VectorClock type
- Implement GossipBridge for peer communication
- Add integration tests for two-peer sync
```