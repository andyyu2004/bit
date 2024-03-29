use proc_macro2::TokenStream;
use quote::quote;
use syn::*;

#[proc_macro_derive(BitObject)]
pub fn derive_bit_object(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let name = input.ident;

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = match input.data {
        Data::Enum(data) => {
            let obj_cached_arms = data.variants.iter().map(|variant| {
                let name = &variant.ident;
                quote! {
                    Self::#name(x) => x.obj_cached(),
                }
            });

            let owner_arms = data.variants.iter().map(|variant| {
                let name = &variant.ident;
                quote! {
                    Self::#name(x) => x.owner(),
                }
            });

            quote! {
                impl #impl_generics libbit::obj::BitObject for #name #ty_generics #where_clause {
                    fn owner(&self) -> BitRepo {
                        match self {
                            #(#owner_arms)*
                        }
                    }

                    fn obj_cached(&self) -> &BitObjCached {
                        match self {
                            #(#obj_cached_arms)*
                        }
                    }
                }
            }
        }
        Data::Struct(..) | Data::Union(..) => panic!("enums only"),
    };

    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(BitArbitrary)]
pub fn derive_bit_quickcheck(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let name = input.ident;

    // Add a bound `T: Quickcheck` to every type parameter T.
    let generics = add_trait_bounds(input.generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let arbitrary = generate_arbitrary_fields(&input.data);

    let expanded = quote! {
        // The generated impl.
        impl #impl_generics quickcheck::Arbitrary for #name #ty_generics #where_clause {
            fn arbitrary(g: &mut quickcheck::Gen) -> Self {
                #arbitrary
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(Merge)]
pub fn derive_merge(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let recurse = fields.named.iter().map(|f| {
                    let name = &f.ident;
                    quote! {
                        self.#name.merge(other.#name)
                    }
                });
                quote! {
                    #(#recurse; )*
                }
            }
            _ => unimplemented!(),
        },
        _ => unimplemented!(),
    };

    let expanded = quote! {
        impl #impl_generics libbit::config::Merge for #name #ty_generics #where_clause {
            fn merge(&mut self, other: Self) {
                #fields
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}

fn generate_arbitrary_fields(data: &Data) -> TokenStream {
    match data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let recurse = fields.named.iter().map(|f| {
                    let name = &f.ident;
                    quote! {
                        #name: quickcheck::Arbitrary::arbitrary(g)
                    }
                });
                quote! {
                    Self {
                        #(#recurse, )*
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let recurse = fields.unnamed.iter().map(|_f| {
                    quote! {
                        quickcheck::Arbitrary::arbitrary(g)
                    }
                });
                quote! {
                    Self(#(#recurse, )*)
                }
            }
            Fields::Unit => todo!(),
        },
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    }
}

// Add a bound `T: QuickCheck` to every type parameter T.
fn add_trait_bounds(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param.bounds.push(parse_quote!(quickcheck::Quickcheck));
        }
    }
    generics
}
