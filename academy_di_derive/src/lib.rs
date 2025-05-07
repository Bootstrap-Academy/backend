use std::collections::HashMap;

use darling::{FromField, util::Flag};
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Field, parse_macro_input};

#[derive(Debug, FromField)]
#[darling(attributes(di))]
struct FieldArgs {
    default: Flag,
}

#[proc_macro_derive(Build, attributes(di))]
pub fn derive_build(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let ident = input.ident;

    let generics = input
        .generics
        .type_params()
        .map(|x| &x.ident)
        .collect::<Vec<_>>();

    let syn::Data::Struct(data) = input.data else {
        return quote! { ::core::compile_error!("Build can only be derived for structs"); }.into();
    };

    let field_args = match data
        .fields
        .iter()
        .map(|f| FieldArgs::from_field(f).map(|args| (f, args)))
        .collect::<Result<HashMap<_, _>, _>>()
    {
        Ok(args) => args,
        Err(err) => return err.write_errors().into(),
    };

    let bounds = data
        .fields
        .iter()
        .filter(|x| !field_args[x].default.is_present())
        .map(|Field { ty, .. }| quote! { #ty: ::academy_di::Build<__Provider> })
        .collect::<Vec<_>>();

    let build_expr = match &data.fields {
        syn::Fields::Named(fields) => {
            let fields = fields
                .named
                .iter()
                .map(|f @ Field { ident, .. }| {
                    if field_args[f].default.is_present() {
                        quote! { #ident: ::core::default::Default::default() }
                    } else {
                        quote! { #ident: ::academy_di::Build::build(provider) }
                    }
                })
                .collect::<Vec<_>>();
            quote! { Self { #(#fields),* } }
        }
        syn::Fields::Unnamed(fields) => {
            let fields = fields
                .unnamed
                .iter()
                .map(|f| {
                    if field_args[f].default.is_present() {
                        quote! { ::core::default::Default::default() }
                    } else {
                        quote! { ::academy_di::Build::build(provider) }
                    }
                })
                .collect::<Vec<_>>();
            quote! { Self( #(#fields),* ) }
        }
        syn::Fields::Unit => quote! { Self },
    };

    quote! {
        impl<__Provider, #(#generics),*> ::academy_di::Build<__Provider> for #ident<#(#generics),*>
        where
            Self: ::core::clone::Clone + 'static,
            __Provider: ::academy_di::Provider,
            #(#bounds),*
        {
            fn build(provider: &mut __Provider) -> Self {
                if let ::core::option::Option::Some(cached) = provider.cache().get().cloned() {
                    cached
                } else {
                    let value = #build_expr;
                    provider.cache().insert(::core::clone::Clone::clone(&value));
                    value
                }
            }
        }
    }
    .into()
}
