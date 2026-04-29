use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Error, Path, Type, parse::Parse, spanned::Spanned};

#[proc_macro_derive(Query, attributes(database, compute))]
pub fn derive_query(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let output = flatten_result(build_query_impl(input));
    output.into()
}

fn flatten_result(result: Result<TokenStream, Error>) -> TokenStream {
    match result {
        Ok(stream) => stream,
        Err(error) => error.into_compile_error(),
    }
}

fn build_query_impl(input: DeriveInput) -> Result<TokenStream, Error> {
    fn required_attr<T: Parse>(attr_name: &str, input: &DeriveInput) -> Result<T, Error> {
        let attr = input
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident(attr_name))
            .ok_or_else(|| {
                Error::new(
                    input.span(),
                    format_args!(
                        "#[derive(Query)] requires the #[{attr_name}] attribute to be present"
                    ),
                )
            })?;

        let parsed = attr.parse_args().map_err(|mut err| {
            err.combine(Error::new(
                attr.span(),
                format_args!("could not parse #[{attr_name}] attribute"),
            ));

            err
        })?;

        Ok(parsed)
    }

    let database: Type = required_attr("database", &input)?;
    let compute: Path = required_attr("compute", &input)?;

    let ident = input.ident;
    let query_impl = quote! {
        ::xh_query::register_database! {
            ::xh_query::database::ErasedDatabase::new::<#database>()
        }

        impl ::xh_query::Query for #ident {
            type Value = <Self::Database as ::xh_query::database::Database>::InputValue;
            type Database = #database;

            fn compute<'a>(self, qcx: &'a ::xh_query::engine::Context<'_>) -> impl Future<Output = Self::Value> + 'a {
                #compute(self, qcx)
            }
        }
    };

    Ok(query_impl)
}
