use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Ident, LitStr, Token, Type, TypePath,
    punctuated::Punctuated,
};

struct NeedEntry {
    name: Ident,
    ty: Type,
    kind_str: String,
    c_str: String,
    i_str: String,
    path: LitStr,
}

impl Parse for NeedEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        input.parse::<Token![=]>()?;
        let path: LitStr = input.parse()?;

        let (kind_str, c_str, i_str) =
            extract_type_info(&ty)?;

        Ok(NeedEntry {
            name,
            ty,
            kind_str,
            c_str,
            i_str,
            path,
        })
    }
}

fn extract_type_info(
    ty: &Type,
) -> syn::Result<(String, String, String)> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "expected File<C, I> or FileTree<C, I>",
        ));
    };
    let seg = path.segments.last().ok_or_else(|| {
        syn::Error::new_spanned(ty, "empty type path")
    })?;
    let kind_str = seg.ident.to_string();
    let syn::PathArguments::AngleBracketed(ref args) =
        seg.arguments
    else {
        return Err(syn::Error::new_spanned(
            ty,
            "expected generic arguments <C, I>",
        ));
    };
    let generics: Vec<_> = args.args.iter().collect();
    if generics.len() != 2 {
        return Err(syn::Error::new_spanned(
            &args.args,
            "expected exactly 2 generic arguments",
        ));
    }
    let c_str = ident_from_generic_arg(generics[0])?;
    let i_str = ident_from_generic_arg(generics[1])?;
    Ok((kind_str, c_str, i_str))
}

fn ident_from_generic_arg(
    arg: &syn::GenericArgument,
) -> syn::Result<String> {
    let syn::GenericArgument::Type(Type::Path(
        TypePath { path, .. },
    )) = arg
    else {
        return Err(syn::Error::new_spanned(
            arg,
            "expected type identifier",
        ));
    };
    let seg = path.segments.last().ok_or_else(|| {
        syn::Error::new_spanned(arg, "empty path")
    })?;
    Ok(seg.ident.to_string())
}

struct NeedsAttr {
    entries: Punctuated<NeedEntry, Token![,]>,
}

impl Parse for NeedsAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let entries = Punctuated::parse_terminated(input)?;
        Ok(NeedsAttr { entries })
    }
}

#[proc_macro_attribute]
pub fn needs(
    attr: TokenStream,
    item: TokenStream,
) -> TokenStream {
    let entries = syn::parse_macro_input!(
        attr as NeedsAttr
    )
    .entries;
    let func = syn::parse_macro_input!(
        item as syn::ItemFn
    );

    let mut statics = Vec::new();
    let mut fields = Vec::new();
    let mut inits = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let static_name = format_ident!(
            "__NEED_{}",
            i
        );
        let path_str = entry.path.value();
        let kind = &entry.kind_str;
        let c = &entry.c_str;
        let ii = &entry.i_str;
        let name = &entry.name;
        let ty = &entry.ty;

        statics.push(quote! {
            #[::linkme::distributed_slice(
                ::jevs::manifest::NEEDS
            )]
            static #static_name: ::jevs::manifest::Need =
                ::jevs::manifest::Need::new(
                    #path_str, #kind, #c, #ii,
                );
        });

        fields.push(quote! {
            pub #name: #ty,
        });

        inits.push(quote! {
            #name: <#ty>::open(key, #path_str),
        });
    }

    let expanded = quote! {
        #(#statics)*

        pub struct Needs {
            #(#fields)*
        }

        pub fn create(
            key: &::jevs::RuntimeKey,
        ) -> Needs {
            Needs {
                #(#inits)*
            }
        }

        #func
    };

    expanded.into()
}
