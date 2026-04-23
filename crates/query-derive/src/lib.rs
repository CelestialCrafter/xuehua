use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    DeriveInput, Error, Expr, Path, Token, Type,
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

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
    struct Database {
        ty: Type,
        expr: Expr,
    }

    impl Parse for Database {
        fn parse(input: ParseStream) -> Result<Self, Error> {
            let ty = input.parse()?;

            let lookahead = input.lookahead1();
            let expr = if lookahead.peek(Token![,]) {
                input.parse::<Token![,]>().and_then(|_| input.parse())
            } else {
                syn::parse2(quote! { Default::default() })
            }?;

            Ok(Self { ty, expr })
        }
    }

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

    let database: Database = required_attr("database", &input)?;
    let compute: Path = required_attr("compute", &input)?;

    let ident = input.ident;
    let db_ty = database.ty;
    let db_expr = database.expr;
    let register = format_ident!("_{ident}_query_register");

    let query_impl = quote! {
        #[::linkme::distributed_slice(::xh_query::engine::REGISTERED_DATABASES)]
        fn #register() -> (::std::any::TypeId, ::std::boxed::Box<dyn ::xh_query::database::DynDatabase>) {
            let db: #db_ty = { #db_expr };
            let type_id = ::std::any::Any::type_id(&db);
            (type_id, Box::new(db) as _)
        }

        impl ::xh_query::Query for #ident {
            type Value = <Self::Database as ::xh_query::database::Database>::InputValue;
            type Database = #db_ty;

            fn compute<'a>(self, qcx: &'a ::xh_query::engine::Context<'_>) -> impl Future<Output = Self::Value> + 'a {
                #compute(self, qcx)
            }
        }
    };

    Ok(query_impl)
}
