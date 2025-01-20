use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Field, Ident, ItemFn};

#[proc_macro_derive(Patch, attributes(no_patch))]
pub fn derive_patch(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let vis = input.vis;
    let ident = input.ident;

    let patch_ident = Ident::new(&format!("{ident}Patch"), ident.span());
    let ref_patch_ident = Ident::new(&format!("{ident}PatchRef"), ident.span());

    let syn::Data::Struct(data) = input.data else {
        return quote! { ::core::compile_error!("Patch can only be derived for structs"); }.into();
    };

    let syn::Fields::Named(fields) = data.fields else {
        return quote! { ::core::compile_error!("Patch can only be derived for structs with named fields"); }.into();
    };

    fn is_no_patch(field: &Field) -> bool {
        field
            .attrs
            .iter()
            .any(|x| x.path().get_ident().is_some_and(|x| x == "no_patch"))
    }

    let patch_fields = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let vis = &field.vis;
            let ident = &field.ident;
            let ty = &field.ty;
            quote! { #vis #ident: ::academy_utils::patch::PatchValue<#ty> }
        })
        .collect::<Vec<_>>();

    let ref_patch_fields = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let vis = &field.vis;
            let ident = &field.ident;
            let ty = &field.ty;
            quote! { #vis #ident: ::academy_utils::patch::PatchValue<&'a #ty> }
        })
        .collect::<Vec<_>>();

    let update_fields = fields
        .named
        .iter()
        .map(|field| {
            let ident = &field.ident;
            if is_no_patch(field) {
                quote! { #ident: self.#ident }
            } else {
                quote! { #ident: patch.#ident.update_or(self.#ident) }
            }
        })
        .collect::<Vec<_>>();

    let into_patch_fields = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: ::academy_utils::patch::PatchValue::Update(self.#ident) }
        })
        .collect::<Vec<_>>();

    let as_patch_ref_fields = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: ::academy_utils::patch::PatchValue::Update(&self.#ident) }
        })
        .collect::<Vec<_>>();

    let as_ref_fields = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: self.#ident.as_ref() }
        })
        .collect::<Vec<_>>();

    let minimize_fields = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: self.#ident.minimize(&old_values.#ident) }
        })
        .collect::<Vec<_>>();

    let ref_minimize_fields = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let ident = &field.ident;
            quote! { #ident: self.#ident.minimize(&&old_values.#ident) }
        })
        .collect::<Vec<_>>();

    let is_update_fields = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let ident = &field.ident;
            quote! { || self.#ident.is_update() }
        })
        .collect::<Vec<_>>();

    let builder_methods = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let ident = field.ident.as_ref().unwrap();
            let update_ident = Ident::new(&format!("update_{ident}"), ident.span());
            let ty = &field.ty;
            quote! {
                #vis fn #update_ident(mut self, #ident: #ty) -> Self {
                    self.#ident = ::academy_utils::patch::PatchValue::Update(#ident);
                    self
                }
            }
        })
        .collect::<Vec<_>>();

    let ref_builder_methods = fields
        .named
        .iter()
        .filter(|x| !is_no_patch(x))
        .map(|field| {
            let ident = field.ident.as_ref().unwrap();
            let update_ident = Ident::new(&format!("update_{ident}"), ident.span());
            let ty = &field.ty;
            quote! {
                #vis fn #update_ident(mut self, #ident: &'a #ty) -> Self {
                    self.#ident = ::academy_utils::patch::PatchValue::Update(#ident);
                    self
                }
            }
        })
        .collect::<Vec<_>>();

    quote! {
        #[derive(::core::fmt::Debug, ::core::clone::Clone, ::core::default::Default, ::core::cmp::PartialEq, ::core::cmp::Eq)]
        #vis struct #patch_ident {
            #(#patch_fields),*
        }

        #[derive(::core::fmt::Debug, ::core::clone::Clone, ::core::marker::Copy, ::core::default::Default, ::core::cmp::PartialEq, ::core::cmp::Eq)]
        #vis struct #ref_patch_ident <'a> {
            #(#ref_patch_fields),*
        }

        impl ::academy_utils::patch::Patch for #ident {
            type Patch = #patch_ident;
            type PatchRef<'a> = #ref_patch_ident <'a>;

            fn update(self, patch: Self::Patch) -> Self {
                Self { #(#update_fields),* }
            }

            fn into_patch(self) -> Self::Patch {
                #patch_ident { #(#into_patch_fields),* }
            }

            fn as_patch_ref(&self) -> Self::PatchRef<'_> {
                #ref_patch_ident { #(#as_patch_ref_fields),* }
            }
        }

        impl #patch_ident {
            #vis fn new() -> Self {
                Self::default()
            }

            #(#builder_methods)*

            #vis fn as_ref(&self) -> #ref_patch_ident<'_> {
                #ref_patch_ident { #(#as_ref_fields),* }
            }

            #vis fn minimize(self, old_values: &#ident) -> Self {
                Self { #(#minimize_fields),* }
            }

            #vis fn is_update(&self) -> bool {
                false #(#is_update_fields)*
            }

            #vis fn is_unchanged(&self) -> bool {
                !self.is_update()
            }
        }

        impl<'a> #ref_patch_ident<'a> {
            #vis fn new() -> Self {
                Self::default()
            }

            #(#ref_builder_methods)*

            #vis fn minimize(self, old_values: &'a #ident) -> Self {
                Self { #(#ref_minimize_fields),* }
            }

            #vis fn is_update(&self) -> bool {
                false #(#is_update_fields)*
            }

            #vis fn is_unchanged(&self) -> bool {
                !self.is_update()
            }
        }
    }
    .into()
}

#[proc_macro_attribute]
pub fn trace_instrument(meta: TokenStream, input: TokenStream) -> TokenStream {
    let meta = proc_macro2::TokenStream::from(meta);
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = parse_macro_input!(input as ItemFn);

    quote! {
        #[::tracing::instrument(ret(level = "trace"), #meta)]
        #(#attrs)*
        #vis #sig {
            ::tracing::trace!("call");
            #block
        }
    }
    .into()
}
