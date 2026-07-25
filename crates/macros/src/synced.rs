use proc_macro::TokenStream;
use quote::quote;
use syn::{
    DeriveInput,
    parse_macro_input,
};

pub fn synced_attribute(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(item as DeriveInput);
    let struct_name = &ast.ident;
    let vis = &ast.vis;
    let attrs = &ast.attrs;
    let generics = &ast.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match &ast.data {
        | syn::Data::Struct(data) => &data.fields,
        | _ => panic!("#[synced] can only be used on structs"),
    };

    TokenStream::from(quote! {
        // Generate the struct with all necessary derives
        #(#attrs)*
        #[derive(::bevy::prelude::Component, Clone, Copy, Debug)]
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

                merge_fn: None,
            }
        }
    })
}
