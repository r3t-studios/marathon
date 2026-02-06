# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-02-06

### Added

#### Core Features
- CRDT-based synchronization using OR-Sets, RGA, and Last-Write-Wins semantics
- Peer-to-peer networking built on iroh with QUIC transport
- Gossip-based message broadcasting for multi-peer coordination
- Offline-first architecture with automatic reconciliation
- SQLite-backed persistence with WAL mode
- Cross-platform support for macOS desktop and iOS

#### Demo Application
- Replicated cube demo showcasing real-time collaboration
- Multiple instance support for local testing
- Apple Pencil input support on iPad
- Real-time cursor and selection synchronization
- Debug UI for inspecting internal state

#### Infrastructure
- Bevy 0.17 ECS integration
- Zero-copy serialization with rkyv
- Automated iOS build tooling via xtask
- Comprehensive RFC documentation covering architecture decisions

### Architecture

- **Networking Layer**: CRDT sync protocol, entity mapping, vector clocks, session management
- **Persistence Layer**: Three-tier system (in-memory → write buffer → SQLite)
- **Engine Core**: Event loop, networking manager, peer discovery, game actions
- **Platform Support**: iOS and desktop with platform-specific input handling

### Documentation

- RFC 0001: CRDT Synchronization Protocol
- RFC 0002: Persistence Strategy
- RFC 0003: Sync Abstraction
- RFC 0004: Session Lifecycle
- RFC 0005: Spatial Audio System
- RFC 0006: Agent Simulation Architecture
- iOS deployment guide
- Estimation methodology documentation

### Known Issues

- API is unstable and subject to change
- Limited documentation for public APIs
- Performance optimizations still needed for large-scale collaboration
- iOS builds require manual Xcode configuration

### Notes

This is an early development release (version 0.x.y). The API is unstable and breaking changes are expected. Not recommended for production use.

[unreleased]: https://github.com/r3t-studios/marathon/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/r3t-studios/marathon/releases/tag/v0.1.0
