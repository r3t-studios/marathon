# AGENTS.md

Guidance for AI coding agents working on the Marathon repository. This file
describes what the project is, how it is built and tested, and the conventions
you are expected to follow. Read it fully before making changes.

## Project Overview

**Marathon** is a peer-to-peer game engine development kit written in Rust,
built on [Bevy 0.17](https://bevyengine.org/) (ECS/game engine) and
[iroh 0.95](https://iroh.computer/) (P2P networking over QUIC). It provides:

- **CRDT-based state synchronization** (OR-Set, RGA, LWW, vector clocks) over
  iroh-gossip, so multiplayer games get offline-first, eventually consistent
  state without a central server.
- **SQLite-backed persistence** (WAL mode, three-tier: in-memory dirty tracking
  → async write buffer → SQLite) with rkyv zero-copy serialization.
- **Cross-platform runtime**: macOS desktop and iOS (simulator and device).

The project is in **early development (< v1.0.0)**; the API is unstable and
breaking changes between minor versions are expected.

The demo game bundled in this workspace is **Aspen**, a replicated-cube
multiplayer app (iOS bundle id `G872CZV7WG.aspen`) with Apple Pencil input
support. The engine itself is `libmarathon`.

Authoritative design documents live in `docs/rfcs/` (0001 CRDT gossip sync,
0002 persistence strategy, 0003 sync abstraction, 0004 session lifecycle, 0005
spatial audio, 0006 agent simulation). `ARCHITECTURE.md` has the full system
overview with diagrams. Consult these before making architectural changes.

## Workspace Layout

Rust workspace (edition 2024, resolver 2) with five crates under `crates/`:

- **`libmarathon`** — the engine library. Key modules under `src/`:
  - `networking/` — CRDT sync protocol: operations, vector clocks, OR-Set/RGA,
    gossip bridge to iroh, sessions, join protocol, `entity_map` (UUID ↔ Bevy
    `Entity` bidirectional mapping), change detection, delta generation.
  - `persistence/` — SQLite database (WAL), type registry (reflection-based,
    populated via `inventory`), migrations, health/metrics, write buffer.
  - `engine/` — `EngineCore` (runs on a tokio runtime in a background thread),
    `EngineBridge` (channel bridge between the Bevy world and EngineCore),
    commands, game actions, peer discovery.
  - `render/` — **vendored** `bevy_render` + `bevy_core_pipeline` + `bevy_pbr`
    (from Bevy v0.17.2, commit `5663583`). Treat this as third-party code:
    keep diffs minimal and intentional.
  - `transform/` — vendored `Transform` with rkyv derives added for network
    serialization.
  - `debug_ui/` — egui-based debug interface integrated into the render
    pipeline.
  - `platform/` — desktop and iOS executors. Marathon **owns the winit event
    loop** (Bevy's `WinitPlugin`/`WindowPlugin`/`InputPlugin` are disabled).
    `platform/ios/` contains a Swift bridge (`PencilCapture.swift`,
    `PencilBridge.h`) for Apple Pencil, compiled by `build.rs` only when
    targeting iOS.
- **`app`** — the Aspen demo game. Library crate types `staticlib`/`cdylib`/`lib`
  (for iOS embedding) plus a `main.rs` binary. Features: `default = ["desktop"]`,
  `ios`, `headless`. Notable modules: `engine_bridge.rs`, `cube.rs`,
  `session.rs`, `input/` (keyboard, touch, Apple Pencil), `setup/control_socket.rs`
  (Unix-domain control socket, default `/tmp/marathon-control.sock`).
- **`macros`** (`libmarathon-macros`) — proc macros. Contains Bevy render macros
  adapted for the vendored render module (`ExtractComponent`, `ExtractResource`,
  `AsBindGroup`, ...) and the `#[synced]` attribute macro, which derives
  `Component + Clone + Copy + rkyv` and registers the component in the
  persistence type registry via `inventory`.
- **`marathonctl`** — CLI (clap + ratatui) that sends `ControlCommand`s to a
  running app instance over its Unix domain socket (`status`, `start
  <session-code>`, `stop`, spawn/delete, ...). Binary name: `marathonctl`.
- **`xtask`** — cargo-xtask build automation for iOS (see below). Invoked via
  the cargo alias in `.cargo/config.toml`: `cargo xtask <cmd>`.

## Build and Run

Prerequisites: Rust (edition 2024 toolchain), macOS for the desktop demo,
Xcode command line tools for iOS. iOS deployment target is 16.0 (set via
`[env]` in `.cargo/config.toml`).

```bash
cargo build                              # build the workspace
cargo run --package app                  # run the desktop demo
cargo run --package app -- --instance 0  # multi-instance multiplayer testing
cargo run --package app -- --db-path x.db --control-socket /tmp/x.sock \
    --log-level debug --no-log-file      # useful CLI flags
```

### iOS

Requires rust targets `aarch64-apple-ios-sim` / `aarch64-apple-ios` and the iOS
simulator runtime. Full guide: `docs/ios-deployment.md`.

```bash
cargo xtask ios-build            # build + package Aspen.app for simulator (release; --debug for debug)
cargo xtask ios-deploy           # install + launch on simulator
cargo xtask ios-run              # build, deploy, and stream OSLogs
cargo xtask ios-device           # build, codesign, install, launch on a connected iPad
```

Equivalent shell scripts live in `scripts/ios/` (`build-simulator.sh`,
`package-app.sh`, `deploy-simulator.sh`); app bundle metadata is
`scripts/ios/Info.plist` and `Entitlements.plist`. Device builds are codesigned
with a hardcoded Apple Development certificate in `crates/xtask/src/main.rs`.

## Testing

The project uses **cargo-nextest** as the preferred runner (`cargo test` also
works):

```bash
cargo nextest run                          # all tests
cargo nextest run --package libmarathon    # one crate
cargo nextest run test_vector_clock_merge  # single test
```

Conventions:

- Unit tests live in `mod tests` blocks in the same file as the code.
- Integration tests live in each crate's `tests/` directory
  (`libmarathon/tests/` has gossip/session/multi-node sync suites plus a
  `test_utils/` helper module; `app/tests/` has headless cube sync tests).
- Property tests use **proptest** (`tests/property_tests.rs`, regression file
  committed as `property_tests.proptest-regressions`).
- Benchmarks use **criterion** (`libmarathon/benches/vector_clock.rs`,
  `write_buffer.rs`; `harness = false`).
- `libmarathon` has a `fast_tests` feature that skips expensive networking
  operations in tests — enable it when you need the suite to run quickly.
- Networking tests spin up real iroh endpoints; prefer the headless/sync test
  harnesses in `tests/test_utils/` over hand-rolled setups.
- Use descriptive test names, e.g. `test_vector_clock_handles_concurrent_updates`.
- Core logic that must stay well-tested: CRDT operations and merge logic,
  persistence write/read paths, network protocol handling.

## Code Style and Conventions

- **Formatting**: `rustfmt.toml` uses unstable options (import granularity
  `Crate`, `StdExternalCrate` grouping, vertical import layout, etc.), so
  format with **`cargo +nightly fmt`**. Format before committing.
- **Linting**: `cargo clippy --workspace --all-targets -- -D warnings` must
  pass cleanly — this is enforced by the pre-commit hook.
- **Docs**: add `///` doc comments for public types, traits, and functions.
  Keep `README.md` and `docs/` in sync with behavior you change. Significant
  architectural changes should update `ARCHITECTURE.md` or add an RFC under
  `docs/rfcs/`.
- **Commits**: strict [Conventional Commits](https://www.conventionalcommits.org)
  (`feat|fix|docs|style|refactor|perf|test|chore|build|ci|revert` with optional
  scope), enforced by a lefthook `commit-msg` hook. `CHANGELOG.md` is generated
  with **git-cliff** (config in `cliff.toml`) from these messages.
- **Git hooks**: managed by [lefthook](https://github.com/evilmartians/lefthook)
  (`lefthook.yml`): pre-commit runs `cargo fmt --check`, clippy with
  `-D warnings`, and a trailing-whitespace check, in parallel.
- **Error handling**: `thiserror` for library error types, `anyhow` at
  application boundaries. Avoid unnecessary `unsafe` (the vendored render code
  is the exception, not the rule).
- **Serialization**: networked/persisted components serialize with **rkyv**;
  the `#[synced]` macro handles the derives and registry registration. Note
  that rkyv archives have no schema evolution — changing a synced component's
  layout breaks existing SQLite blobs (known issue, see `backlog.md`).
- Follow the Rust API Guidelines; prefer composition and use the type system
  to enforce invariants. Match the surrounding module's existing style.

## Runtime Architecture Notes

Understanding these will save you from common mistakes:

- **Two worlds, one bridge**: the Bevy app runs on the main thread; `EngineCore`
  runs on a tokio runtime in a spawned thread. They communicate through
  `EngineBridge` (crossbeam channels). Never call async networking code
  directly from Bevy systems — go through the bridge/commands.
- **Marathon owns the event loop**: `DefaultPlugins` is used with
  `WinitPlugin`, `WindowPlugin`, `InputPlugin`, and `GilrsPlugin` disabled;
  `libmarathon::platform::run_executor(app)` drives the loop. Input arrives via
  Marathon's own `InputEvent` pipeline.
- **Entity identity**: Bevy `Entity` ids are local and differ per instance;
  networked entities are identified by stable UUIDs via
  `networking/entity_map.rs`. Always go through the mapping for anything that
  crosses the wire.
- **Sync model**: local changes generate immutable CRDT ops (with vector
  clocks) that are applied locally, broadcast via gossip, and marked dirty for
  persistence. Remote ops are gated on vector-clock causality before apply.
  Replication is currently whole-component LWW — see `backlog.md` for known
  limitations and planned work.
- **Persistence**: components registered in the type registry (via `#[synced]`
  / `inventory`) persist automatically. The app sets
  `PersistenceConfig { flush_interval_secs: 2, checkpoint_interval_secs: 30,
  battery_adaptive: true, .. }`.
- **Vendored code**: `src/render/` and `src/transform/` are vendored from Bevy
  0.17.2 with local adaptations (e.g. rkyv derives, `crate::render` macro
  paths). Make only deliberate, minimal edits there and note them in the
  module-header comment style already in use.

## Environment and Secrets

- The project uses **direnv**: create a gitignored `.envrc` with
  `export GH_TOKEN=...` (scopes `repo`, `security_events`) for GitHub tooling.
  Never commit `.envrc` or any credential.
- There is **no CI** at the moment: `.github/` contains only issue and PR
  templates, no workflows. All verification is local (fmt, clippy, tests).
- `Cargo.lock` is intentionally gitignored (library convention).
- The root `config.toml` is tracked in git but appears to be a stray machine
  config from a different project (embedding models, tailscale, gRPC); it is
  not read by any crate in this workspace. Do not rely on it.

## Security Considerations

- Report vulnerabilities privately per `SECURITY.md` — never in public issues.
- Current trust model (per `ARCHITECTURE.md`): **all peers are trusted** — no
  authentication, no authorization, no encryption beyond QUIC transport
  security. Do not assume otherwise when writing networking code.
- SQLite files (`*.db`, `*.db-wal`, `*.db-shm`), logs, and `.env*` files are
  gitignored; keep it that way.
- The control socket (`/tmp/marathon-control.sock`) accepts commands from any
  local process; treat it as a debug interface, not a hardened API.

## AI Usage Policy

`AI_POLICY.md` applies to all contributions, including agent-generated ones:
a human must make the architectural decisions, understand every submitted
line, and be accountable for the result. AI tools are welcome assistants but
may not make design decisions or be credited as contributors. When unsure,
the policy's litmus test is: *"Can I maintain and debug this?"*
