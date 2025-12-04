//! CRDT-based networking layer for distributed synchronization
//!
//! This module implements the networking strategy defined in RFC 0001.
//! It provides CRDT-based synchronization over iroh-gossip with support for:
//!
//! - **Entity Synchronization** - Automatic sync of NetworkedEntity components
//! - **CRDT Merge Semantics** - LWW, OR-Set, and Sequence CRDTs
//! - **Large Blob Support** - Files >64KB via iroh-blobs
//! - **Join Protocol** - New peers receive full world state
//! - **Anti-Entropy** - Periodic sync to repair network partitions
//! - **Vector Clock** - Causality tracking for distributed operations
//!
//! # Example
//!
//! ```
//! use lib::networking::*;
//! use uuid::Uuid;
//!
//! // Create a vector clock and track operations
//! let node_id = Uuid::new_v4();
//! let mut clock = VectorClock::new();
//!
//! // Increment the clock for local operations
//! clock.increment(node_id);
//!
//! // Build a component operation
//! let builder = ComponentOpBuilder::new(node_id, clock.clone());
//! let op = builder.set("Transform".to_string(), ComponentData::Inline(vec![1, 2, 3]));
//! ```

mod apply_ops;
mod blob_support;
mod change_detection;
mod components;
mod delta_generation;
mod entity_map;
mod error;
mod gossip_bridge;
mod join_protocol;
mod merge;
mod message_dispatcher;
mod messages;
mod operation_builder;
mod operation_log;
mod operations;
mod orset;
mod plugin;
mod rga;
mod sync_component;
mod tombstones;
mod vector_clock;

pub use apply_ops::*;
pub use blob_support::*;
pub use change_detection::*;
pub use components::*;
pub use delta_generation::*;
pub use entity_map::*;
pub use error::*;
pub use gossip_bridge::*;
pub use join_protocol::*;
pub use merge::*;
pub use message_dispatcher::*;
pub use messages::*;
pub use operation_builder::*;
pub use operation_log::*;
pub use operations::*;
pub use orset::*;
pub use plugin::*;
pub use rga::*;
pub use sync_component::*;
pub use tombstones::*;
pub use vector_clock::*;
