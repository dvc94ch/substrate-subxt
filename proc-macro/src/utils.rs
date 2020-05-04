// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of substrate-subxt.
//
// subxt is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// subxt is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with substrate-subxt.  If not, see <http://www.gnu.org/licenses/>.

use proc_macro2::{
    Span,
    TokenStream,
};
use quote::quote;
use syn::{
    parse::{
        Parse,
        ParseStream,
    },
    punctuated::Punctuated,
};
use synstructure::{
    BindingInfo,
    Structure,
};

pub fn use_crate(name: &str) -> syn::Ident {
    opt_crate(name).unwrap_or_else(|| syn::Ident::new("crate", Span::call_site()))
}

pub fn opt_crate(name: &str) -> Option<syn::Ident> {
    proc_macro_crate::crate_name(name)
        .ok()
        .map(|krate| syn::Ident::new(&krate, Span::call_site()))
}

pub fn bindings<'a>(s: &'a Structure) -> Vec<&'a BindingInfo<'a>> {
    let mut bindings = vec![];
    for variant in s.variants() {
        for binding in variant.bindings() {
            bindings.push(binding);
        }
    }
    bindings
}

pub fn ident_to_name(ident: &syn::Ident, ty: &str) -> String {
    let name = ident.to_string();
    let name = name.trim_end_matches(ty);
    if name.is_empty() {
        ty.to_string()
    } else {
        name.to_string()
    }
}

pub fn module_name(generics: &syn::Generics) -> &syn::Path {
    generics
        .params
        .iter()
        .filter_map(|p| {
            if let syn::GenericParam::Type(p) = p {
                p.bounds
                    .iter()
                    .filter_map(|b| {
                        if let syn::TypeParamBound::Trait(t) = b {
                            Some(&t.path)
                        } else {
                            None
                        }
                    })
                    .next()
            } else {
                None
            }
        })
        .next()
        .unwrap()
}

pub fn path_to_ident(path: &syn::Path) -> &syn::Ident {
    &path.segments.iter().last().unwrap().ident
}

pub fn type_params(generics: &syn::Generics) -> Vec<TokenStream> {
    generics
        .params
        .iter()
        .filter_map(|g| {
            match g {
                syn::GenericParam::Type(p) => {
                    let ident = &p.ident;
                    Some(quote!(#ident))
                }
                syn::GenericParam::Lifetime(p) => {
                    let lifetime = &p.lifetime;
                    Some(quote!(#lifetime))
                }
                syn::GenericParam::Const(_) => None,
            }
        })
        .collect()
}

#[derive(Debug)]
pub struct Attrs<A> {
    pub paren: syn::token::Paren,
    pub attrs: Punctuated<A, syn::token::Comma>,
}

impl<A: Parse> Parse for Attrs<A> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        Ok(Self {
            paren: syn::parenthesized!(content in input),
            attrs: content.parse_terminated(A::parse)?,
        })
    }
}

#[derive(Debug)]
pub struct Attr<K, V> {
    pub key: K,
    pub eq: syn::token::Eq,
    pub value: V,
}

impl<K: Parse, V: Parse> Parse for Attr<K, V> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            key: input.parse()?,
            eq: input.parse()?,
            value: input.parse()?,
        })
    }
}

#[cfg(test)]
pub(crate) fn assert_proc_macro(
    result: proc_macro2::TokenStream,
    expected: proc_macro2::TokenStream,
) {
    let result = result.to_string();
    let expected = expected.to_string();
    pretty_assertions::assert_eq!(result, expected);
}
