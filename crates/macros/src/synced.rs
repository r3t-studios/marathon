use proc_macro::TokenStream;
use quote::quote;
use syn::{
    DeriveInput,
    Expr,
    MetaNameValue,
    parse_macro_input,
};

pub fn synced_attribute(attr: TokenStream, item: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(item as DeriveInput);
    let struct_name = &ast.ident;
    let vis = &ast.vis;
    let attrs = &ast.attrs;
    let generics = &ast.generics;
    let (_impl_generics, ty_generics, _where_clause) = generics.split_for_impl();

    let fields = match &ast.data {
        | syn::Data::Struct(data) => &data.fields,
        | _ => panic!("#[synced] can only be used on structs"),
    };

    // Parse optional `merge = path::to::merge_fn`
    //
    // The value must be a path to a function with the ComponentMeta::merge_fn
    // signature, e.g. #[synced(merge = libmarathon::networking::merge_into::<T>)].
    let merge_fn_tokens = if attr.is_empty() {
        quote! { merge_fn: None, }
    } else {
        let meta = parse_macro_input!(attr as MetaNameValue);
        if !meta.path.is_ident("merge") {
            return syn::Error::new_spanned(
                &meta.path,
                "unsupported #[synced] option; expected `merge = path::to::merge_fn`",
            )
            .to_compile_error()
            .into();
        }
        match &meta.value {
            | Expr::Path(path) => quote! { merge_fn: Some(#path), },
            | other => {
                return syn::Error::new_spanned(
                    other,
                    "`merge` must be a path to a merge function, e.g. \
                     #[synced(merge = libmarathon::networking::merge_into::<T>)]",
                )
                .to_compile_error()
                .into();
            },
        }
    };

    TokenStream::from(quote! {
        // Generate the struct with all necessary derives
        #(#attrs)*
        #[derive(::bevy::prelude::Component, Clone, Debug)]
        #[derive(::rkyv::Archive, ::rkyv::Serialize, ::rkyv::Deserialize)]
        #vis struct #struct_name #generics #fields

        // Register component in type registry using inventory
        ::inventory::submit! {
            ::libmarathon::persistence::ComponentMeta {
                type_name: stringify!(#struct_name),
                type_path: concat!(module_path!(), "::", stringify!(#struct_name)),
                type_id: std::any::TypeId::of::<#struct_name>(),

                deserialize_fn: |bytes: &[u8]| -> anyhow::Result<Box<dyn std::any::Any>> {
                    let component = ::rkyv::from_bytes::<#struct_name #ty_generics, ::rkyv::rancor::Failure>(bytes)?;
                    Ok(Box::new(component))
                },

                serialize_fn: |world: &::bevy::ecs::world::World, entity: ::bevy::ecs::entity::Entity|
                    -> Option<::bytes::Bytes>
                {
                    world.get::<#struct_name #ty_generics>(entity).map(|component| {
                        let serialized = ::rkyv::to_bytes::<::rkyv::rancor::Failure>(component)
                            .expect("Failed to serialize component");
                        ::bytes::Bytes::from(serialized.to_vec())
                    })
                },

                insert_fn: |entity_mut: &mut ::bevy::ecs::world::EntityWorldMut,
                           boxed: Box<dyn std::any::Any>|
                {
                    if let Ok(component) = boxed.downcast::<#struct_name #ty_generics>() {
                        entity_mut.insert(*component);
                    }
                },

                #merge_fn_tokens
            }
        }

        // Register for per-type change detection: edits to this component
        // mark the entity for delta generation (see ChangeDetectionMeta)
        ::inventory::submit! {
            ::libmarathon::networking::ChangeDetectionMeta {
                type_name: stringify!(#struct_name),
                system: {
                    // fn item (not a call — statics can't call); the builder
                    // runs when the system executes, not at registration
                    fn detect(world: &mut ::bevy::ecs::world::World) {
                        ::libmarathon::networking::build_change_detection_system::<#struct_name #ty_generics>()(world)
                    }
                    detect
                },
            }
        }
    })
}
