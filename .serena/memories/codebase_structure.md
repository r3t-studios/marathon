# Codebase Structure

## Workspace Organization
```
aspen/
├── crates/
│   ├── app/              # Main application
│   │   ├── src/
│   │   │   ├── main.rs         # Entry point
│   │   │   ├── camera.rs       # Camera system
│   │   │   ├── cube.rs         # 3D cube demo
│   │   │   ├── debug_ui.rs     # Debug overlays
│   │   │   ├── engine_bridge.rs # Bridge to EngineCore
│   │   │   ├── input/          # Input handling
│   │   │   ├── rendering.rs    # Rendering setup
│   │   │   ├── selection.rs    # Object selection
│   │   │   ├── session.rs      # Session management
│   │   │   ├── session_ui.rs   # Session UI
│   │   │   └── setup.rs        # App initialization
│   │   └── Cargo.toml
│   │
│   ├── libmarathon/      # Core library
│   │   ├── src/
│   │   │   ├── lib.rs          # Library root
│   │   │   ├── sync.rs         # Synchronization primitives
│   │   │   ├── engine/         # Core engine logic
│   │   │   ├── networking/     # P2P networking, gossip
│   │   │   ├── persistence/    # Database and storage
│   │   │   ├── platform/       # Platform-specific code
│   │   │   │   ├── desktop/    # macOS executor
│   │   │   │   └── ios/        # iOS executor
│   │   │   └── debug_ui/       # Debug UI components
│   │   └── Cargo.toml
│   │
│   ├── sync-macros/      # Procedural macros for sync
│   │   └── src/lib.rs
│   │
│   └── xtask/            # Build automation
│       ├── src/main.rs
│       └── README.md
│
├── scripts/
│   └── ios/              # iOS-specific build scripts
│       ├── Info.plist          # iOS app metadata
│       ├── Entitlements.plist  # App capabilities
│       ├── deploy-simulator.sh # Simulator deployment
│       └── build-simulator.sh  # Build for simulator
│
├── docs/
│   └── rfcs/             # Architecture RFCs
│       ├── README.md
│       ├── 0001-crdt-gossip-sync.md
│       ├── 0002-persistence-strategy.md
│       ├── 0003-sync-abstraction.md
│       ├── 0004-session-lifecycle.md
│       ├── 0005-spatial-audio-system.md
│       └── 0006-agent-simulation-architecture.md
│
├── .github/
│   └── ISSUE_TEMPLATE/   # GitHub issue templates
│       ├── bug_report.yml
│       ├── feature.yml
│       ├── task.yml
│       ├── epic.yml
│       └── support.yml
│
├── Cargo.toml            # Workspace configuration
├── Cargo.lock            # Dependency lock file
└── rustfmt.toml          # Code formatting rules
```

## Key Patterns
- **ECS Architecture**: Uses Bevy's Entity Component System
- **Platform Abstraction**: Separate executors for desktop/iOS
- **Engine-UI Separation**: `EngineCore` runs in background thread, communicates via `EngineBridge`
- **CRDT-based Sync**: All shared state uses CRDTs for conflict-free merging
- **RFC-driven Design**: Major decisions documented in `docs/rfcs/`