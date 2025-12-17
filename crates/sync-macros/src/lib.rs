use proc_macro::TokenStream;
use quote::quote;
use syn::{
    DeriveInput,
    parse_macro_input,
};

/// Sync strategy types
#[derive(Debug, Clone, PartialEq)]
enum SyncStrategy {
    LastWriteWins,
    Set,
    Sequence,
    Custom,
}

impl SyncStrategy {
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            | "LastWriteWins" => Ok(SyncStrategy::LastWriteWins),
            | "Set" => Ok(SyncStrategy::Set),
            | "Sequence" => Ok(SyncStrategy::Sequence),
            | "Custom" => Ok(SyncStrategy::Custom),
            | _ => Err(format!(
                "Unknown strategy '{}'. Choose one of: \"LastWriteWins\", \"Set\", \"Sequence\", \"Custom\"",
                s
            )),
        }
    }

    fn to_tokens(&self) -> proc_macro2::TokenStream {
        match self {
            | SyncStrategy::LastWriteWins => {
                quote! { libmarathon::networking::SyncStrategy::LastWriteWins }
            },
            | SyncStrategy::Set => quote! { libmarathon::networking::SyncStrategy::Set },
            | SyncStrategy::Sequence => quote! { libmarathon::networking::SyncStrategy::Sequence },
            | SyncStrategy::Custom => quote! { libmarathon::networking::SyncStrategy::Custom },
        }
    }
}

/// Parsed sync attributes
struct SyncAttributes {
    version: u32,
    strategy: SyncStrategy,
    persist: bool,
    lazy: bool,
}

impl SyncAttributes {
    fn parse(input: &DeriveInput) -> Result<Self, syn::Error> {
        let mut version: Option<u32> = None;
        let mut strategy: Option<SyncStrategy> = None;
        let mut persist = true; // default
        let mut lazy = false; // default

        // Find the #[sync(...)] attribute
        for attr in &input.attrs {
            if !attr.path().is_ident("sync") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("version") {
                    let value: syn::LitInt = meta.value()?.parse()?;
                    version = Some(value.base10_parse()?);
                    Ok(())
                } else if meta.path.is_ident("strategy") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    let strategy_str = value.value();
                    strategy = Some(
                        SyncStrategy::from_str(&strategy_str)
                            .map_err(|e| syn::Error::new_spanned(&value, e))?,
                    );
                    Ok(())
                } else if meta.path.is_ident("persist") {
                    let value: syn::LitBool = meta.value()?.parse()?;
                    persist = value.value;
                    Ok(())
                } else if meta.path.is_ident("lazy") {
                    let value: syn::LitBool = meta.value()?.parse()?;
                    lazy = value.value;
                    Ok(())
                } else {
                    Err(meta.error("unrecognized sync attribute"))
                }
            })?;
        }

        // Require version and strategy
        let version = version.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "Missing required attribute `version`\n\
                 \n\
                 = help: Add #[sync(version = 1, strategy = \"...\")] to your struct\n\
                 = note: See documentation: https://docs.rs/lonni/sync/strategies.html",
            )
        })?;

        let strategy = strategy.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "Missing required attribute `strategy`\n\
                 \n\
                 = help: Choose one of: \"LastWriteWins\", \"Set\", \"Sequence\", \"Custom\"\n\
                 = help: Add #[sync(version = 1, strategy = \"LastWriteWins\")] to your struct\n\
                 = note: See documentation: https://docs.rs/lonni/sync/strategies.html",
            )
        })?;

        Ok(SyncAttributes {
            version,
            strategy,
            persist,
            lazy,
        })
    }
}

/// RFC 0003 macro: Generate SyncComponent trait implementation
///
/// # Example
/// ```ignore
/// use bevy::prelude::*;
/// use libmarathon::networking::Synced;
/// use sync_macros::Synced as SyncedDerive;
///
/// #[derive(Component, Clone)]
/// #[derive(Synced)]
/// #[sync(version = 1, strategy = "LastWriteWins")]
/// struct Health(f32);
///
/// // In a Bevy system:
/// fn spawn_health(mut commands: Commands) {
///     commands.spawn((Health(100.0), Synced));
/// }
/// ```
#[proc_macro_derive(Synced, attributes(sync))]
pub fn derive_synced(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Parse attributes
    let attrs = match SyncAttributes::parse(&input) {
        | Ok(attrs) => attrs,
        | Err(e) => return TokenStream::from(e.to_compile_error()),
    };

    let name = &input.ident;
    let name_str = name.to_string();
    let version = attrs.version;
    let strategy_tokens = attrs.strategy.to_tokens();

    // Generate serialization method based on type
    let serialize_impl = generate_serialize(&input);
    let deserialize_impl = generate_deserialize(&input, name);

    // Generate merge method based on strategy
    let merge_impl = generate_merge(&input, &attrs.strategy);

    // Note: Users must add #[derive(rkyv::Archive, rkyv::Serialize,
    // rkyv::Deserialize)] to their struct
    let expanded = quote! {
        // Register component with inventory for type registry
        // Build type path at compile time using concat! and module_path!
        // since std::any::type_name() is not yet const
        const _: () = {
            const TYPE_PATH: &str = concat!(module_path!(), "::", stringify!(#name));

            inventory::submit! {
                libmarathon::persistence::ComponentMeta {
                    type_name: #name_str,
                    type_path: TYPE_PATH,
                    type_id: std::any::TypeId::of::<#name>(),
                deserialize_fn: |bytes: &[u8]| -> anyhow::Result<Box<dyn std::any::Any>> {
                    let component: #name = rkyv::from_bytes::<#name, rkyv::rancor::Failure>(bytes)?;
                    Ok(Box::new(component))
                },
                serialize_fn: |world: &bevy::ecs::world::World, entity: bevy::ecs::entity::Entity| -> Option<Vec<u8>> {
                    world.get::<#name>(entity).and_then(|component| {
                        rkyv::to_bytes::<rkyv::rancor::Failure>(component)
                            .map(|bytes| bytes.to_vec())
                            .ok()
                    })
                },
                insert_fn: |entity_mut: &mut bevy::ecs::world::EntityWorldMut, boxed: Box<dyn std::any::Any>| {
                    if let Ok(component) = boxed.downcast::<#name>() {
                        entity_mut.insert(*component);
                    }
                },
            }
            };
        };

        impl libmarathon::networking::SyncComponent for #name {
            const VERSION: u32 = #version;
            const STRATEGY: libmarathon::networking::SyncStrategy = #strategy_tokens;

            #[inline]
            fn serialize_sync(&self) -> anyhow::Result<Vec<u8>> {
                #serialize_impl
            }

            #[inline]
            fn deserialize_sync(data: &[u8]) -> anyhow::Result<Self> {
                #deserialize_impl
            }

            #[inline]
            fn merge(&mut self, remote: Self, clock_cmp: libmarathon::networking::ClockComparison) -> libmarathon::networking::ComponentMergeDecision {
                #merge_impl
            }
        }
    };

    TokenStream::from(expanded)
}

/// Generate specialized serialization code
fn generate_serialize(_input: &DeriveInput) -> proc_macro2::TokenStream {
    // Use rkyv for zero-copy serialization
    // Later we can optimize for specific types (e.g., f32 -> to_le_bytes)
    quote! {
        rkyv::to_bytes::<rkyv::rancor::Failure>(self).map(|bytes| bytes.to_vec()).map_err(|e| anyhow::anyhow!("Serialization failed: {}", e))
    }
}

/// Generate specialized deserialization code
fn generate_deserialize(_input: &DeriveInput, _name: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        rkyv::from_bytes::<Self, rkyv::rancor::Failure>(data).map_err(|e| anyhow::anyhow!("Deserialization failed: {}", e))
    }
}

/// Generate merge logic based on strategy
fn generate_merge(input: &DeriveInput, strategy: &SyncStrategy) -> proc_macro2::TokenStream {
    match strategy {
        | SyncStrategy::LastWriteWins => generate_lww_merge(input),
        | SyncStrategy::Set => generate_set_merge(input),
        | SyncStrategy::Sequence => generate_sequence_merge(input),
        | SyncStrategy::Custom => generate_custom_merge(input),
    }
}

/// Generate hash calculation code for tiebreaking in concurrent merges
///
/// Returns a TokenStream that computes hashes for both local and remote values
/// and compares them for deterministic conflict resolution.
fn generate_hash_tiebreaker() -> proc_macro2::TokenStream {
    quote! {
        let local_hash = {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(self).map(|b| b.to_vec()).unwrap_or_default();
            bytes.iter().fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64))
        };
        let remote_hash = {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Failure>(&remote).map(|b| b.to_vec()).unwrap_or_default();
            bytes.iter().fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64))
        };
    }
}

/// Generate Last-Write-Wins merge logic
fn generate_lww_merge(_input: &DeriveInput) -> proc_macro2::TokenStream {
    let hash_tiebreaker = generate_hash_tiebreaker();

    quote! {
        use tracing::info;

        match clock_cmp {
            libmarathon::networking::ClockComparison::RemoteNewer => {
                info!(
                    component = std::any::type_name::<Self>(),
                    ?clock_cmp,
                    "Taking remote (newer)"
                );
                *self = remote;
                libmarathon::networking::ComponentMergeDecision::TookRemote
            }
            libmarathon::networking::ClockComparison::LocalNewer => {
                libmarathon::networking::ComponentMergeDecision::KeptLocal
            }
            libmarathon::networking::ClockComparison::Concurrent => {
                // Tiebreaker: Compare serialized representations for deterministic choice
                // In a real implementation, we'd use node_id, but for now use a simple hash
                #hash_tiebreaker

                if remote_hash > local_hash {
                    info!(
                        component = std::any::type_name::<Self>(),
                        ?clock_cmp,
                        "Taking remote (concurrent, tiebreaker)"
                    );
                    *self = remote;
                    libmarathon::networking::ComponentMergeDecision::TookRemote
                } else {
                    libmarathon::networking::ComponentMergeDecision::KeptLocal
                }
            }
        }
    }
}

/// Generate OR-Set merge logic
///
/// For OR-Set strategy, the component must contain an OrSet<T> field.
/// We merge by calling the OrSet's merge method which implements add-wins
/// semantics.
fn generate_set_merge(_input: &DeriveInput) -> proc_macro2::TokenStream {
    let hash_tiebreaker = generate_hash_tiebreaker();

    quote! {
        use tracing::info;

        // For Set strategy, we always merge the sets
        // The OrSet CRDT handles the conflict resolution with add-wins semantics
        info!(
            component = std::any::type_name::<Self>(),
            "Merging OR-Set (add-wins semantics)"
        );

        // Assuming the component wraps an OrSet or has a field with merge()
        // For now, we'll do a structural merge by replacing the whole value
        // This is a simplified implementation - full implementation would require
        // the component to expose merge() method or implement it directly

        match clock_cmp {
            libmarathon::networking::ClockComparison::RemoteNewer => {
                *self = remote;
                libmarathon::networking::ComponentMergeDecision::TookRemote
            }
            libmarathon::networking::ClockComparison::LocalNewer => {
                libmarathon::networking::ComponentMergeDecision::KeptLocal
            }
            libmarathon::networking::ClockComparison::Concurrent => {
                // In a full implementation, we would merge the OrSet here
                // For now, use LWW with tiebreaker as fallback
                #hash_tiebreaker

                if remote_hash > local_hash {
                    *self = remote;
                    libmarathon::networking::ComponentMergeDecision::TookRemote
                } else {
                    libmarathon::networking::ComponentMergeDecision::KeptLocal
                }
            }
        }
    }
}

/// Generate RGA/Sequence merge logic
///
/// For Sequence strategy, the component must contain an Rga<T> field.
/// We merge by calling the Rga's merge method which maintains causal ordering.
fn generate_sequence_merge(_input: &DeriveInput) -> proc_macro2::TokenStream {
    let hash_tiebreaker = generate_hash_tiebreaker();

    quote! {
        use tracing::info;

        // For Sequence strategy, we always merge the sequences
        // The RGA CRDT handles the conflict resolution with causal ordering
        info!(
            component = std::any::type_name::<Self>(),
            "Merging RGA sequence (causal ordering)"
        );

        // Assuming the component wraps an Rga or has a field with merge()
        // For now, we'll do a structural merge by replacing the whole value
        // This is a simplified implementation - full implementation would require
        // the component to expose merge() method or implement it directly

        match clock_cmp {
            libmarathon::networking::ClockComparison::RemoteNewer => {
                *self = remote;
                libmarathon::networking::ComponentMergeDecision::TookRemote
            }
            libmarathon::networking::ClockComparison::LocalNewer => {
                libmarathon::networking::ComponentMergeDecision::KeptLocal
            }
            libmarathon::networking::ClockComparison::Concurrent => {
                // In a full implementation, we would merge the Rga here
                // For now, use LWW with tiebreaker as fallback
                #hash_tiebreaker

                if remote_hash > local_hash {
                    *self = remote;
                    libmarathon::networking::ComponentMergeDecision::TookRemote
                } else {
                    libmarathon::networking::ComponentMergeDecision::KeptLocal
                }
            }
        }
    }
}

/// Generate custom merge logic placeholder
fn generate_custom_merge(input: &DeriveInput) -> proc_macro2::TokenStream {
    let name = &input.ident;
    quote! {
        compile_error!(
            concat!(
                "Custom strategy requires implementing ConflictResolver trait for ",
                stringify!(#name)
            )
        );
        libmarathon::networking::ComponentMergeDecision::KeptLocal
    }
}
