use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, Data, Fields, Type, ItemStruct};

/// Attribute macro for transparent CRDT sync
///
/// Transforms your struct to use CRDTs internally while keeping the API simple.
///
/// # Example
/// ```
/// #[synced]
/// struct EmotionGradientConfig {
///     canvas_width: f32,        // Becomes SyncedValue<f32> internally
///     canvas_height: f32,       // Auto-generates getters/setters
///
///     #[sync(skip)]
///     node_id: String,          // Not synced
/// }
///
/// // Use it like a normal struct:
/// let mut config = EmotionGradientConfig::new("node1".into());
/// config.set_canvas_width(1024.0);  // Auto-generates sync operation
/// println!("Width: {}", config.canvas_width());  // Transparent access
/// ```
#[proc_macro_attribute]
pub fn synced(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;
    let vis = &input.vis;
    let op_enum_name = format_ident!("{}Op", name);

    let fields = match &input.fields {
        Fields::Named(fields) => &fields.named,
        _ => panic!("synced only supports structs with named fields"),
    };

    let mut internal_fields = Vec::new();
    let mut field_getters = Vec::new();
    let mut field_setters = Vec::new();
    let mut op_variants = Vec::new();
    let mut apply_arms = Vec::new();
    let mut merge_code = Vec::new();
    let mut new_params = Vec::new();
    let mut new_init = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_vis = &field.vis;
        let field_type = &field.ty;

        // Check if field should be skipped
        let should_skip = field.attrs.iter().any(|attr| {
            attr.path().is_ident("sync")
                && attr
                    .parse_args::<syn::Ident>()
                    .map(|i| i == "skip")
                    .unwrap_or(false)
        });

        if should_skip {
            // Keep as-is, no wrapping
            internal_fields.push(quote! {
                #field_vis #field_name: #field_type
            });
            new_params.push(quote! { #field_name: #field_type });
            new_init.push(quote! { #field_name });
            continue;
        }

        // Wrap in SyncedValue
        internal_fields.push(quote! {
            #field_name: lib::sync::SyncedValue<#field_type>
        });

        // Generate getter
        field_getters.push(quote! {
            #field_vis fn #field_name(&self) -> &#field_type {
                self.#field_name.get()
            }
        });

        // Generate setter that returns operation
        let setter_name = format_ident!("set_{}", field_name);
        let op_variant = format_ident!(
            "Set{}",
            field_name
                .to_string()
                .chars()
                .enumerate()
                .map(|(i, c)| if i == 0 {
                    c.to_ascii_uppercase()
                } else {
                    c
                })
                .collect::<String>()
        );

        field_setters.push(quote! {
            #field_vis fn #setter_name(&mut self, value: #field_type) -> #op_enum_name {
                let op = #op_enum_name::#op_variant {
                    value: value.clone(),
                    timestamp: chrono::Utc::now(),
                    node_id: self.node_id().clone(),
                };
                self.#field_name.set(value, self.node_id().clone());
                op
            }
        });

        // Generate operation variant
        op_variants.push(quote! {
            #op_variant {
                value: #field_type,
                timestamp: chrono::DateTime<chrono::Utc>,
                node_id: String,
            }
        });

        // Generate apply arm
        apply_arms.push(quote! {
            #op_enum_name::#op_variant { value, timestamp, node_id } => {
                self.#field_name.apply_lww(value.clone(), timestamp.clone(), node_id.clone());
            }
        });

        // Generate merge code
        merge_code.push(quote! {
            self.#field_name.merge(&other.#field_name);
        });

        // Add to new() parameters
        new_params.push(quote! { #field_name: #field_type });
        new_init.push(quote! {
            #field_name: lib::sync::SyncedValue::new(#field_name, node_id.clone())
        });
    }

    let expanded = quote! {
        /// Sync operations enum
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        #[serde(tag = "type")]
        #vis enum #op_enum_name {
            #(#op_variants),*
        }

        impl #op_enum_name {
            pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
                Ok(serde_json::to_vec(self)?)
            }

            pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
                Ok(serde_json::from_slice(bytes)?)
            }
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        #vis struct #name {
            #(#internal_fields),*
        }

        impl #name {
            #vis fn new(#(#new_params),*) -> Self {
                Self {
                    #(#new_init),*
                }
            }

            /// Transparent field accessors
            #(#field_getters)*

            /// Field setters that generate sync operations
            #(#field_setters)*

            /// Apply a sync operation from another node
            #vis fn apply_op(&mut self, op: &#op_enum_name) {
                match op {
                    #(#apply_arms),*
                }
            }

            /// Merge state from another instance
            #vis fn merge(&mut self, other: &Self) {
                #(#merge_code)*
            }
        }

        impl lib::sync::Syncable for #name {
            type Operation = #op_enum_name;

            fn apply_sync_op(&mut self, op: &Self::Operation) {
                self.apply_op(op);
            }

            fn node_id(&self) -> &lib::sync::NodeId {
                // Assume there's a node_id field marked with #[sync(skip)]
                &self.node_id
            }
        }
    };

    TokenStream::from(expanded)
}

/// Old derive macro - kept for backwards compatibility
#[proc_macro_derive(Synced, attributes(sync))]
pub fn derive_synced(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let op_enum_name = format_ident!("{}Op", name);

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("Synced only supports structs with named fields"),
        },
        _ => panic!("Synced only supports structs"),
    };

    let mut field_ops = Vec::new();
    let mut apply_arms = Vec::new();
    let mut setter_methods = Vec::new();
    let mut merge_code = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;

        // Check if field should be skipped
        let should_skip = field.attrs.iter()
            .any(|attr| {
                attr.path().is_ident("sync") &&
                attr.parse_args::<syn::Ident>()
                    .map(|i| i == "skip")
                    .unwrap_or(false)
            });

        if should_skip {
            continue;
        }

        let op_variant = format_ident!("Set{}",
            field_name.to_string()
                .chars()
                .enumerate()
                .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
                .collect::<String>()
        );

        let setter_name = format_ident!("set_{}", field_name);

        // Determine CRDT strategy based on type
        let crdt_strategy = get_crdt_strategy(field_type);

        match crdt_strategy.as_str() {
            "lww" => {
                // LWW for simple types
                field_ops.push(quote! {
                    #op_variant {
                        value: #field_type,
                        timestamp: chrono::DateTime<chrono::Utc>,
                        node_id: String,
                    }
                });

                apply_arms.push(quote! {
                    #op_enum_name::#op_variant { value, timestamp, node_id } => {
                        self.#field_name.apply_lww(value.clone(), timestamp.clone(), node_id.clone());
                    }
                });

                setter_methods.push(quote! {
                    pub fn #setter_name(&mut self, value: #field_type) -> #op_enum_name {
                        let op = #op_enum_name::#op_variant {
                            value: value.clone(),
                            timestamp: chrono::Utc::now(),
                            node_id: self.node_id().clone(),
                        };
                        self.#field_name = lib::sync::SyncedValue::new(value, self.node_id().clone());
                        op
                    }
                });

                merge_code.push(quote! {
                    self.#field_name.merge(&other.#field_name);
                });
            }
            _ => {
                // Default to LWW
            }
        }
    }

    let expanded = quote! {
        /// Auto-generated sync operations enum
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        #[serde(tag = "type")]
        pub enum #op_enum_name {
            #(#field_ops),*
        }

        impl #op_enum_name {
            pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
                Ok(serde_json::to_vec(self)?)
            }

            pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
                Ok(serde_json::from_slice(bytes)?)
            }
        }

        impl #name {
            /// Apply a sync operation from another node
            pub fn apply_op(&mut self, op: &#op_enum_name) {
                match op {
                    #(#apply_arms),*
                }
            }

            /// Merge state from another instance
            pub fn merge(&mut self, other: &Self) {
                #(#merge_code)*
            }

            /// Auto-generated setter methods that create sync ops
            #(#setter_methods)*
        }

        impl lib::sync::Syncable for #name {
            type Operation = #op_enum_name;

            fn apply_sync_op(&mut self, op: &Self::Operation) {
                self.apply_op(op);
            }
        }
    };

    TokenStream::from(expanded)
}

/// Determine CRDT strategy based on field type
fn get_crdt_strategy(_ty: &Type) -> String {
    // For now, default everything to LWW
    // TODO: Detect HashMap -> use Map, Vec -> use ORSet, etc.
    "lww".to_string()
}
