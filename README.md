---
title: Marathon
---

# Marathon

[![Matrix](https://img.shields.io/badge/chat-%23hello%3Asunbeam.pt-0dbd8b?logo=matrix)](https://matrix.to/#/#hello:sunbeam.pt)
[![License](https://img.shields.io/github/license/sunbeamdotpt/marathon)](LICENSE.md)

**A peer-to-peer game engine development kit built with Rust and CRDTs**

Marathon is a multiplayer game engine framework designed for building real-time collaborative games with offline-first capabilities. Built on [Bevy](https://bevyengine.org/) and [iroh](https://iroh.computer/), it provides CRDT-based state synchronization, peer-to-peer networking, and persistent state management out of the box - so you can focus on making great games instead of wrestling with networking code.

## ⚠️ Early Development Notice

**This project is in early development (<v1.0.0).**

- The API is unstable and may change without notice
- Breaking changes are expected between minor versions
- Not recommended for production use
- Documentation is still being written
- We welcome feedback and contributions!

Version 1.0.0 will indicate the first stable release.

## Features

- **CRDT-based synchronization** - Conflict-free multiplayer state management using OR-Sets, RGA, and LWW semantics
- **Peer-to-peer networking** - Built on iroh with QUIC transport and gossip-based message broadcasting
- **Offline-first architecture** - Players can continue playing during network issues and sync when reconnected
- **Persistent game state** - SQLite-backed storage with automatic entity persistence
- **Cross-platform** - Supports macOS desktop and iOS (simulator and device), with more platforms planned
- **Built with Bevy** - Leverages the Bevy game engine's ECS architecture and parts of its ecosystem

## Demo

The current demo is a **replicated cube game** that synchronizes in real-time across multiple instances:
- Apple Pencil input support on iPad
- Real-time player cursor and selection sharing
- Automatic game state synchronization across network partitions
- Multiple players can interact with the same game world simultaneously

## Quick Start

### Prerequisites

- **Rust** 2024 edition or later
- **macOS** (for desktop demo)
- **Xcode** (for iOS development)

### Building the Desktop Demo

```bash
# Clone the repository
git clone https://github.com/yourusername/marathon.git
cd marathon

# Build and run
cargo run --package app
```

To run multiple instances for testing multiplayer:

```bash
# Terminal 1
cargo run --package app -- --instance 0

# Terminal 2
cargo run --package app -- --instance 1
```

### Building for iOS

Marathon includes automated iOS build tooling via `xtask`:

```bash
# Build for iOS simulator
cargo xtask ios-build

# Deploy to connected device
cargo xtask ios-deploy

# Build and run on simulator
cargo xtask ios-run
```

See [docs/ios-deployment.md](docs/ios-deployment.md) for detailed iOS setup instructions.

## Architecture

Marathon is organized as a Rust workspace with four crates:

- **`libmarathon`** - Core engine library with networking, persistence, and CRDT sync
- **`app`** - Demo game showcasing multiplayer cube gameplay
- **`macros`** - Procedural macros for serialization
- **`xtask`** - Build automation for iOS deployment

### Key Components

- **Networking** - CRDT synchronization protocol, gossip-based broadcast, entity mapping
- **Persistence** - Three-tier system (in-memory → write buffer → SQLite WAL)
- **Engine** - Core event loop, peer discovery, session management
- **Debug UI** - egui-based debug interface for inspecting state

For detailed architecture information, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Documentation

- **[RFCs](docs/rfcs/)** - Design documents covering core architecture decisions
  - [0001: CRDT Synchronization Protocol](docs/rfcs/0001-crdt-gossip-sync.md)
  - [0002: Persistence Strategy](docs/rfcs/0002-persistence-strategy.md)
  - [0003: Sync Abstraction](docs/rfcs/0003-sync-abstraction.md)
  - [0004: Session Lifecycle](docs/rfcs/0004-session-lifecycle.md)
  - [0005: Spatial Audio System](docs/rfcs/0005-spatial-audio-system.md)
  - [0006: Agent Simulation Architecture](docs/rfcs/0006-agent-simulation-architecture.md)
- **[iOS Deployment Guide](docs/ios-deployment.md)** - Complete iOS build instructions
- **[Estimation Methodology](docs/ESTIMATION.md)** - Project sizing and prioritization approach

## Technology Stack

- **[Bevy 0.17](https://bevyengine.org/)** - Game engine and ECS framework
- **[iroh 0.95](https://iroh.computer/)** - P2P networking with QUIC
- **[iroh-gossip 0.95](https://github.com/n0-computer/iroh-gossip)** - Gossip protocol for multi-peer coordination
- **[SQLite](https://www.sqlite.org/)** - Local persistence with WAL mode
- **[rkyv 0.8](https://rkyv.org/)** - Zero-copy serialization
- **[egui 0.33](https://www.egui.rs/)** - Immediate-mode GUI
- **[wgpu 26](https://wgpu.rs/)** - Graphics API (via Bevy)

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Development environment setup
- Code style and conventions
- How to submit pull requests
- Testing guidelines

Please also read our [Code of Conduct](CODE_OF_CONDUCT.md) and [AI Usage Policy](AI_POLICY.md).

## Project Status

Marathon is actively developed and currently focused on:
- Core CRDT synchronization protocol
- Persistence layer stability
- Multi-platform support (macOS, iOS)
- Demo applications

See our [roadmap](https://github.com/yourusername/marathon/issues) for planned features and known issues.

## Community

- **Issues** - [GitHub Issues](https://github.com/yourusername/marathon/issues)
- **Discussions** - [GitHub Discussions](https://github.com/yourusername/marathon/discussions)

## Security

Please see our [Security Policy](SECURITY.md) for information on reporting vulnerabilities.

## License

Marathon is licensed under the [MIT License](LICENSE).

## Acknowledgments

Marathon builds on the incredible work of:
- The [Bevy community](https://bevyengine.org/) for the game engine
- The [iroh team](https://iroh.computer/) for P2P networking infrastructure
- The Rust CRDT ecosystem

---

**Built with Rust 🦀 and collaborative spirit**
