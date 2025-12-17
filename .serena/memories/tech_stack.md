# Tech Stack

## Language
- **Rust** (Edition 2021)
- Some Swift bridging code for iOS-specific features (Apple Pencil)

## Key Dependencies

### Networking & Synchronization
- **iroh** (v0.95) - P2P networking and NAT traversal
- **iroh-gossip** (v0.95) - Gossip protocol for message propagation
- **crdts** (v7.3) - Conflict-free Replicated Data Types

### Graphics & UI
- **Bevy** (v0.17) - Game engine for rendering and ECS architecture
- **egui** (v0.33) - Immediate mode GUI
- **wgpu** - Low-level GPU API
- **winit** (v0.30) - Window handling

### Storage & Persistence
- **rusqlite** (v0.37) - SQLite database bindings
- **serde** / **serde_json** - Serialization
- **bincode** - Binary serialization

### Async Runtime
- **tokio** (v1) - Async runtime with full features
- **futures-lite** (v2.0) - Lightweight futures utilities

### Utilities
- **anyhow** / **thiserror** - Error handling
- **tracing** / **tracing-subscriber** - Structured logging
- **uuid** - Unique identifiers
- **chrono** - Date/time handling
- **rand** (v0.8) - Random number generation
- **crossbeam-channel** - Multi-producer multi-consumer channels

### iOS-Specific
- **objc** (v0.2) - Objective-C runtime bindings
- **tracing-oslog** (v0.3) - iOS unified logging integration
- **raw-window-handle** (v0.6) - Platform window abstractions

### Development Tools
- **clap** - CLI argument parsing (in xtask)
- **criterion** - Benchmarking
- **proptest** - Property-based testing
- **tempfile** - Temporary file handling in tests