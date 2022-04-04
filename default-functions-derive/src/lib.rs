use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput};

#[proc_macro_derive(DefaultFunctions)]
pub fn derive_default_functions(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match input.data {
        Data::Struct(s) => {
            let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
            let struct_ident = input.ident;
            let gen_methods = s.fields.iter().map(|f| {
                if let Some(field_ident) = &f.ident {
                    let gen_method_ident = format_ident!("default_{}", field_ident);
                    let ty = &f.ty;
                    let vis = &f.vis;
                    quote! {
                        #vis fn #gen_method_ident() -> #ty {
                            <Self as ::core::default::Default>::default().#field_ident
                        }
                    }
                } else {
                    quote! {
                        ::std::compile_error!("This macro only accepts named structs (not tuple structs!).");
                    }
                }
            });

            quote! {

                impl #impl_generics #struct_ident #ty_generics #where_clause {
                    #(#gen_methods)*
                }
            }
        }
        Data::Enum(_) => quote! {
            ::std::compile_error!("This macro only accepts structs (found: enum)");
        },
        Data::Union(_) => quote! {
            ::std::compile_error!("This macro only accepts structs (found: union)");
        },
    }.into()
}
