# RFC 0003: Sync Abstraction Layer - "Never Think About It Again"

**Status:** Draft
**Authors:** Sienna
**Created:** 2025-11-16
**Related:** RFC 0001 (CRDT Sync Protocol), RFC 0002 (Persistence Strategy for Battery-Efficient State Management)

## Abstract

This RFC proposes a high-level abstraction layer that makes CRDT synchronization, persistence, and networking feel effortless. Using compile-time code generation and explicit configuration, it provides ergonomic developer experience without sacrificing observability, performance, or debuggability.

**Design Principle:** "Organize complexity clearly, don't hide it."

## Motivation

### The Problem We're Solving

Even with a complete CRDT implementation (RFC 0001) and battery-efficient persistence (RFC 0002), building multiplayer features still requires deep knowledge of:

- CRDT semantics and when to use which type
- NetworkedEntity components and UUID management
- Manual change detection and delta generation
- Vector clock causality tracking
- Operation log management and anti-entropy
- Persistence lifecycle and flush timing
- Blob threshold decisions

**This is too much.** Application developers just want their data to sync. They shouldn't need a PhD in distributed systems.

### The Dream Experience

Imagine you're building a collaborative drawing app. You want the canvas to sync between peers. Here's what it should look like:

```rust
#[derive(Component, Reflect, Synced)]
#[sync(version = 1, strategy = "LastWriteWins")]
struct Canvas {
    strokes: Vec<Stroke>,
    background: Color,
}

fn spawn_canvas(mut commands: Commands) {
    commands.spawn((
        Canvas {
            strokes: vec![],
            background: Color::WHITE,
        },
        Synced,  // ← Marker component
    ));
}

fn draw_stroke(mut canvas: Query<&mut Canvas>) {
    for mut canvas in &mut canvas {
        canvas.strokes.push(new_stroke);  // ← Automatically syncs to peers
    }
}
```

**What's happening behind the scenes:**
- The `#[derive(Synced)]` macro generates zero-cost serialization and merge code
- The `#[sync(version = 1, strategy = "LastWriteWins")]` explicitly defines sync behavior
- The generated code is visible via `cargo expand` for inspection and debugging
- Change detection uses Bevy's native system (zero overhead)

You write declarative configuration, the macro generates explicit, inspectable code.

## How It Works: Two-Tier Architecture

The abstraction provides two levels of API that work together:

### Tier 1: Convenience Layer (What You Write)

```rust
#[derive(Component, Reflect, Synced)]
#[sync(version = 1, strategy = "LastWriteWins")]
struct Health(f32);

commands.spawn((Health(100.0), Synced));
```

**Developer experience:** Declarative attributes, minimal boilerplate, clear intent.

### Tier 2: Generated Implementation (What Gets Compiled)

The `#[derive(Synced)]` macro generates explicit, zero-cost code:

```rust
// Generated trait implementation (visible via `cargo expand`)
impl SyncComponent for Health {
    const VERSION: u32 = 1;
    const STRATEGY: SyncStrategy = SyncStrategy::LastWriteWins;

    #[inline]  // Zero-cost abstraction
    fn serialize_sync(&self) -> Result<Vec<u8>> {
        // Specialized, fast serialization
        Ok(self.0.to_le_bytes().to_vec())
    }

    #[inline]
    fn deserialize_sync(data: &[u8]) -> Result<Self> {
        // Specialized, fast deserialization
        let bytes: [u8; 4] = data.try_into()?;
        Ok(Health(f32::from_le_bytes(bytes)))
    }

    #[inline]
    fn merge(&mut self, remote: Self, clock_cmp: ClockComparison) -> MergeDecision {
        // Explicit, inlined LWW logic
        match clock_cmp {
            ClockComparison::RemoteNewer => {
                *self = remote;
                MergeDecision::TookRemote { logged: true }
            }
            ClockComparison::LocalNewer => MergeDecision::KeptLocal,
            ClockComparison::Concurrent => {
                // Tiebreaker: higher node ID wins
                if remote.node_id > self.node_id {
                    *self = remote;
                    MergeDecision::TookRemote { logged: true }
                } else {
                    MergeDecision::KeptLocal
                }
            }
        }
    }
}

// Generated change detection system
pub fn detect_health_changes(
    changed: Query<(Entity, &Health), (Changed<Health>, With<Synced>)>,
    mut sync_queue: ResMut<SyncQueue>,
) {
    for (entity, health) in &changed {
        sync_queue.push(entity, health.serialize_sync().unwrap());
    }
}
```

**Benefits:**
- **Explicit:** Every operation is a concrete method call, not runtime reflection
- **Inspectable:** `cargo expand` shows exactly what code runs
- **Zero-cost:** Inlined, specialized code with no dynamic dispatch
- **Debuggable:** Stack traces show real function names, not reflection internals
- **Expert-friendly:** Advanced users can write `SyncComponent` manually for full control

## Configuration: Explicit Over Implicit

### Required Attributes

Every synced component **must** specify two things:

1. **Version number** - for schema evolution
2. **Sync strategy** - explicit CRDT selection

```rust
#[derive(Component, Reflect, Synced)]
#[sync(version = 1, strategy = "LastWriteWins")]  // ← REQUIRED
struct Health(f32);
```

**Attempting to omit either causes a compile error:**

```rust
#[derive(Component, Reflect, Synced)]
#[sync(version = 1)]  // ← Missing strategy
struct Health(f32);

// Compile error:
// error: Missing required attribute `strategy`
//   --> src/components.rs:3:1
//    |
// 3  | #[sync(version = 1)]
//    | ^^^^^^^^^^^^^^^^^^^^
//    |
//    = help: Choose one of: "LastWriteWins", "Set", "Sequence", "Custom"
//    = note: See documentation: https://docs.rs/lonni/sync/strategies.html
```

**Rationale:** Inference is error-prone. Requiring explicit decisions forces developers to think about semantics and prevents subtle bugs months later.

### Strategy Selection Guide

**"LastWriteWins"** - Simple values with single correct state
```rust
#[sync(version = 1, strategy = "LastWriteWins")]
struct Position { x: f32, y: f32 }
```
- Use for: positions, health, colors, timestamps
- Concurrent edits: newer write wins (node ID tiebreaker)
- Data loss: yes, losing write is discarded

**"Set"** - Unordered collections
```rust
#[sync(version = 1, strategy = "Set")]
struct Tags(HashSet<String>);
```
- Use for: tags, selections, unordered sets
- Concurrent adds: both appear
- Concurrent add/remove: add wins
- Data loss: no, eventually consistent

**"Sequence"** - Ordered collections
```rust
#[sync(version = 1, strategy = "Sequence")]
struct Path(Vec<Point>);
```
- Use for: ordered lists, text, drawing strokes
- Concurrent inserts: both appear in consistent order
- Position tracking: RGA-based (Replicated Growable Array)
- Data loss: no, all operations preserved

**"Custom"** - Your own conflict resolution
```rust
#[sync(version = 1, strategy = "Custom")]
struct Score(u32);

impl ConflictResolver for Score {
    fn resolve(&self, remote: &Self, context: &ConflictContext) -> Self {
        Score(self.0.max(remote.0))  // Always take maximum
    }
}
```
- Use for: domain-specific merging (max, min, union)
- Requires: `ConflictResolver` trait implementation
- Compile error if trait not implemented

### Optional Attributes

```rust
#[sync(
    version = 1,
    strategy = "LastWriteWins",
    persist = false,      // Don't save to disk (default: true)
    access = "Private",   // Access control policy (default: "Public")
    lazy = true,          // Lazy load large data (default: false)
)]
struct EphemeralCursor(Vec2);
```


## Core Design Principles

### 1. Explicit Over Implicit

Requiring explicit `strategy` attributes prevents subtle bugs. When a developer writes `Vec<Item>`, we can't reliably infer whether they want ordered sequence semantics (RGA) or unordered set semantics. Asking explicitly is better than guessing wrong.

### 2. Macro Codegen for Zero-Cost Abstraction

Generate specialized code at compile time rather than using runtime reflection. Macros produce zero-cost inlined code that's as fast as hand-written serialization while maintaining convenience. Generated code is visible via `cargo expand`, making debugging straightforward.

### 3. Observability from Day One

Every sync operation logs what happened. Every merge decision is traceable. Developers can always answer "Why didn't this sync?" Logging, tracing, and inspection are core features from Phase 1, not afterthoughts.

### 4. Mandatory Schema Versioning

Schema evolution isn't an edge case—it's inevitable. Applications change, data structures evolve. Making versioning mandatory from day one prevents data corruption before it happens.

### 5. Performance Accountability

**Commitment:** <1% frame time for 1000 entities with 10 synced components at 60 FPS.
**Measurement:** Benchmark before Phase 1 ships.
**Consequence:** If target missed, optimize or redesign.

## Schema Evolution (Required, Not Optional)

Adding fields to a component is inevitable. The abstraction handles this through required versioning:

```rust
// Version 1
#[sync(version = 1, strategy = "LastWriteWins")]
struct Player {
    name: String,
    health: f32,
}

// Version 2 - adding a field
#[sync(version = 2, strategy = "LastWriteWins")]
struct Player {
    name: String,
    health: f32,
    level: u32,  // New field
}

// Manual migration (full control)
impl MigrateSyncComponent for Player {
    fn migrate(from_version: u32, data: &[u8]) -> Result<Self> {
        match from_version {
            1 => {
                let v1 = bincode::deserialize::<PlayerV1>(data)?;
                Ok(Player {
                    name: v1.name,
                    health: v1.health,
                    level: 1,  // Explicit default
                })
            }
            2 => bincode::deserialize(data),
            v => Err(SyncError::UnsupportedVersion(v)),
        }
    }
}
```

**Future ergonomic improvement** (Phase 3):
```rust
// Attribute-based migration for simple additive changes
#[sync(version = 2, strategy = "LastWriteWins")]
#[migrate_from(version = 1, default_fields = ["level"])]
struct Player {
    name: String,
    health: f32,
    #[default = 1]
    level: u32,
}
// Generates migration automatically for common case
```

**Enforcement:**
- Incrementing version without migration → compile error
- Receiving data from future version → logged error, graceful degradation
- Receiving data from ancient version (>30 days old) → reject with clear message

**Rationale:** Schema evolution causes data corruption if ignored. Making it mandatory from day one prevents disasters.

## Core Features

### Access Control

**Use case:** Private components that shouldn't sync to all peers.

```rust
#[sync(version = 1, strategy = "LastWriteWins", access = "PrivateToOwner")]
struct PrivateNote {
    content: String,
    owner_id: UserId,
}

impl AccessPolicy for PrivateNote {
    fn can_sync_to(&self, peer: &PeerId) -> bool {
        peer.user_id == self.owner_id
    }
}
```

**Implementation:** Authentication via iroh's cryptographic NodeIds, authorization checks before broadcasting.

### Atomic Operations

**Use case:** Multiple entities that must appear together (player + equipment).

```rust
// High-level transaction API (recommended)
commands.atomic_sync_transaction(|tx| {
    let player = tx.spawn((Player { name: "Alice".into() }, Synced));
    tx.spawn((Weapon { damage: 10, equipped_by: player }, Synced));
    // Both entities broadcast atomically when closure exits
});
```

**How it works:**
- Transaction reserves stable IDs automatically
- Spawns are buffered during closure execution
- On closure exit, all spawns broadcast as a single atomic message
- Remote peers receive both entities or neither (atomic guarantee)

**Low-level API** (for advanced use cases):
```rust
let player_id = SyncId::reserve();
let sword_id = SyncId::reserve();
commands.spawn((Player { /* ... */ }, player_id.clone(), Synced));
commands.spawn((Weapon { equipped_by: player_id }, sword_id, Synced));
commands.broadcast_atomic(vec![
    AtomicOp::Spawn(player_id),
    AtomicOp::Spawn(sword_id),
]);
```

**Implementation:** ID reservation system, batched broadcast primitive, rollback on failure.

### Lazy Loading

**Use case:** Large documents (>1MB) where loading everything upfront is expensive.

```rust
#[sync(version = 1, strategy = "LastWriteWins", lazy = true)]
struct LargeDocument {
    metadata: DocumentMetadata,  // Always in memory

    #[sync(lazy)]
    content: String,  // Loaded on access
}
```

**Implementation:** Blob storage backend, LRU eviction, explicit load semantics.

## Performance Characteristics

**Change detection:** Bevy's native `Changed<T>` filter (virtually zero overhead).

**Serialization:** Macro-generated specialized code (~2-5ns per component, comparable to hand-written).

**Merge logic:** Inlined, branch-predicted (~1-2ns per operation).

**Binary size:** Moderate increase (~10KB per synced component type).

**Benchmark target:** <1% frame time for 1000 entities with 10 synced components at 60 FPS.

## Observability: No Black Boxes

Every sync operation is logged and traceable. When something doesn't work, developers can always find out why.

**Built-in diagnostics:**
- Every merge decision logged: `[SYNC] Health: TookRemote (vector clock: remote=5 > local=3)`
- Trace mode shows full merge logic execution
- `cargo expand` reveals generated code
- Bevy inspector shows sync metadata (network ID, vector clock, last sync time)

**Diagnostic tool:**
```rust
commands.entity(entity).insert(DiagnoseSync);
```

This checks: Is entity marked `Synced`? Has component changed? Is network connected? Are operations queued? When was last successful sync?

**Principle:** Debugging is first-class, not an afterthought.

## Implementation Phases

### Phase 1: Macro Foundation
- Implement `#[derive(Synced)]` macro with required `version` and `strategy` attributes
- Generate `SyncComponent` trait implementations (serialize/deserialize/merge)
- Generate change detection systems per component type
- Basic LWW, Set, and Sequence strategies
- Inspector integration for sync metadata
- Diagnostic logging for all sync operations
- Benchmark and validate <1% overhead target

**Goal:** Macro generates correct, zero-cost code. Debugging tools work.

### Phase 2: Core Features
- Schema migration framework (required `MigrateSyncComponent` on version bump)
- Access control policies
- Atomic operations (ID reservation, batched broadcast)
- Lazy loading for large components
- Anti-entropy and partition recovery
- Comprehensive error handling

**Goal:** Production-ready for real applications. Handles schema changes gracefully.

### Phase 3: Polish
- Improved error messages from macro (guide developers to correct strategy)
- Visual sync state in Bevy editor plugin
- Performance profiling tools
- Migration helper CLI
- Comprehensive examples and documentation

**Goal:** Delightful developer experience. Self-documenting APIs.

## Success Metrics

1. **Developer velocity:** New developers add multiplayer to existing game in <2 hours
2. **Code clarity:** Sync-related code is < 5% of application code (just attributes)
3. **Debugging effectiveness:** "Why didn't this sync?" questions answered via `DiagnoseSync` in <5 minutes
4. **Performance:** Measured overhead stays <1% frame time for benchmark workload
5. **Reliability:** Schema migrations work correctly 100% of the time (enforced by compiler)

## Design Decisions and Open Questions

### Strategy Migration

**Question:** If a component changes from LWW to RGA strategy, how do we migrate existing persisted data?

**Answer:** This is a **destructive change** to the data's underlying semantics and cannot be automated safely. The RFC's stance:

**Strategy migration is a manual, one-off process:**

1. Changing strategy is a fundamental change in data meaning (e.g., "last value wins" → "preserve all insertions")
2. Developers must write a migration script to:
   - Read old data (using old strategy)
   - Transform it to new representation
   - Write back (using new strategy)
3. This is similar to changing a SQL column from `VARCHAR` to `JSON[]` - requires manual intervention

**Example migration script:**
```rust
// One-off migration: Position from LWW to RGA (hypothetical)
fn migrate_position_lww_to_rga() {
    let old_positions = load_all::<Position, LWWStrategy>();
    for (id, pos) in old_positions {
        let rga_pos = Position::from_lww(pos);  // Manual conversion
        save_with_strategy::<Position, RGAStrategy>(id, rga_pos);
    }
}
```

**Rationale:** Automating strategy migration would hide the semantic implications and likely produce incorrect results. Better to be explicit that this is a major change requiring careful thought.

### Open Questions

1. **Conflict visualization:** Should applications optionally be notified when conflicts occur for UI feedback? (e.g., showing a "merged with remote changes" indicator)
2. **Partial sync:** Should we support syncing only specific fields of a component, or is component-level granularity sufficient?
3. **Migration tooling:** Should we provide a CLI tool to scaffold migration functions automatically based on field diffs?

## Why This Matters

**RFC 0001** provides the foundation: a correct, robust CRDT synchronization protocol.

**RFC 0003** provides the interface: an ergonomic abstraction that makes the foundation accessible.

The goal isn't to hide complexity—it's to **organize it clearly**. Developers get:
- Declarative configuration (via attributes)
- Generated implementation (via macros)
- Observable behavior (via logging and diagnostics)
- Performance guarantees (via benchmarks)
- Correctness enforcement (via required versioning)

They can prototype in single-player, add sync attributes, and have a working multiplayer prototype. When things break, they have tools to understand why.

## References

- RFC 0001: CRDT Synchronization Protocol
- RFC 0002: Persistence Strategy
- [Automerge](https://automerge.org/): Inspiration for ergonomic sync

## Quick Reference

**Minimal example:**
```rust
#[derive(Component, Reflect, Synced)]
#[sync(version = 1, strategy = "LastWriteWins")]
struct Health(f32);

commands.spawn((Health(100.0), Synced));
```

**Strategy selection:**
- `"LastWriteWins"` - simple values (positions, health)
- `"Set"` - unordered collections (tags, selections)
- `"Sequence"` - ordered lists (paths, text, strokes)
- `"Custom"` - implement `ConflictResolver`

**Debugging:**
```rust
commands.entity(entity).insert(DiagnoseSync);
```

**Inspection:**
```rust
cargo expand  // View generated code
```

That's the essence: declarative attributes generate explicit, observable, zero-cost code.
