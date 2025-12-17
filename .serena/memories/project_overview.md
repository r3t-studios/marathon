# Project Overview: Aspen

## Purpose
Aspen (formerly known as Lonni) is a **cross-platform real-time collaborative application** built for macOS and iPad. It demonstrates real-time CRDT (Conflict-free Replicated Data Type) synchronization with Apple Pencil input support.

## Key Features
- Real-time collaborative drawing/interaction with Apple Pencil support
- P2P synchronization using CRDTs over iroh-gossip protocol
- Cross-platform: macOS desktop and iOS/iPadOS
- 3D rendering using Bevy game engine
- Persistent local storage with SQLite
- Session management for multi-user collaboration

## Target Platforms
- **macOS** (desktop application)
- **iOS/iPadOS** (with Apple Pencil support)
- Uses separate executors for each platform

## Architecture
The application uses a **workspace structure** with multiple crates:
- `app` - Main application entry point and UI
- `libmarathon` - Core library with engine, networking, persistence
- `sync-macros` - Procedural macros for synchronization
- `xtask` - Build automation tasks

## Development Status
Active development with RFCs for major design decisions. See `docs/rfcs/` for architectural documentation.