use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Error, Fields, Ident, LitStr, Member, Token, parse_macro_input,
    punctuated::Punctuated,
};

enum FrameType {
    Suggestion,
    Attachment,
    Context,
}

#[proc_macro_derive(IntoReport, attributes(suggestion, attachment, context, format))]
pub fn derive_into_report(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let output = build_impl(&input.ident, build_frames(&input));

    output.into()
}

fn build_frames<'a>(input: &DeriveInput) -> TokenStream {
    fn build_frames<'a>(
        fields: &Fields,
        instructions: impl Iterator<Item = (FrameType, &'a Attribute)>,
    ) -> impl Iterator<Item = TokenStream> {
        instructions.map(move |(ty, attr)| {
            flatten_result(match ty {
                FrameType::Suggestion => build_suggestion(fields, attr),
                FrameType::Attachment => build_attachment(attr),
                FrameType::Context => build_context(attr),
            })
        })
    }

    match &input.data {
        Data::Struct(data) => {
            let instructions = build_instructions(&input.attrs);
            let frames = build_frames(&data.fields, instructions);
            let bindings = data.fields.members().map(|member| {
                let escaped = escape_member(member.clone());
                quote! { let #escaped  = &self.#member; }
            });

            quote! {
                #(#bindings)*
                [#(#frames),*]
            }
        }
        Data::Enum(data) => {
            let arms = data.variants.iter().map(|variant| {
                let instructions = build_instructions(&variant.attrs);
                let frames = build_frames(&variant.fields, instructions);

                let members = variant.fields.members().map(escape_member);
                let bindings = match &variant.fields {
                    Fields::Named(_) => quote! { {#(ref #members),*} },
                    Fields::Unnamed(_) => quote! { (#(ref #members),*) },
                    Fields::Unit => TokenStream::new(),
                };

                let enum_ident = &input.ident;
                let variant_ident = &variant.ident;

                let alloc = alloc();
                quote! { #enum_ident::#variant_ident #bindings => #alloc vec![#(#frames),*]}
            });

            quote! { match self { #(#arms),* } }
        }
        Data::Union(_) => Error::new(
            input.ident.span(),
            "#[derive(IntoReport)] does not support unions",
        )
        .to_compile_error(),
    }
}

fn build_impl(ident: &Ident, frames: TokenStream) -> TokenStream {
    quote! {
        impl ::xh_reports::IntoReport for #ident {
            fn into_report(self) -> Report<Self> {
                #frames.into_iter().fold(
                    Report::new(self),
                    |acc, x| acc.with_frame(x)
                )
            }
        }
    }
}

fn build_instructions<'a>(
    attrs: &'a [Attribute],
) -> impl Iterator<Item = (FrameType, &'a Attribute)> {
    attrs.iter().filter_map(|attr| {
        let path = attr.path();
        let ty = if path.is_ident("suggestion") {
            FrameType::Suggestion
        } else if path.is_ident("attachment") {
            FrameType::Attachment
        } else if path.is_ident("context") {
            FrameType::Context
        } else {
            return None;
        };

        Some((ty, attr))
    })
}

fn build_suggestion(fields: &Fields, attr: &Attribute) -> Result<TokenStream, Error> {
    let fmt: LitStr = attr.parse_args()?;
    let members: Vec<_> = fields
        .iter()
        .filter(|field| {
            field
                .attrs
                .iter()
                .find(|attr| attr.path().is_ident("format"))
                .is_some()
        })
        .enumerate()
        .map(|(i, field)| match field.ident.clone() {
            Some(ident) => ident.into(),
            None => i.into(),
        })
        .collect();

    let bindings = match fields {
        Fields::Named(_) => quote! { #(#members = #members),* },
        Fields::Unnamed(_) => {
            let members = members.into_iter().map(escape_member);
            quote! { #(#members),* }
        }
        Fields::Unit => TokenStream::new(),
    };

    let alloc = alloc();
    Ok(quote! {{
        use #alloc::borrow::Cow;

        let args = format_args!(#fmt, #bindings);
        let string = match args.as_str() {
            Some(string) => Cow::Borrowed(string),
            None => Cow::Owned(args.to_string())
        };

        ::xh_reports::Frame::suggestion(string)
    }})
}

fn build_attachment(attr: &Attribute) -> Result<TokenStream, Error> {
    let member = escape_member(attr.parse_args()?);
    Ok(quote! { ::xh_reports::Frame::attachment(#member) })
}

fn build_context(attr: &Attribute) -> Result<TokenStream, Error> {
    let (keys, values): (Vec<_>, Vec<_>) = attr
        .parse_args_with(Punctuated::<Member, Token![,]>::parse_terminated)?
        .into_iter()
        .map(|member| {
            let (string, span) = match &member {
                Member::Named(ident) => (ident.to_string(), ident.span()),
                Member::Unnamed(index) => (index.index.to_string(), index.span),
            };

            (LitStr::new(&string, span), escape_member(member))
        })
        .unzip();

    Ok(quote! { ::xh_reports::Frame::context([#((#keys, format_args!("{:?}", #values))),*]) })
}

fn flatten_result(result: Result<TokenStream, Error>) -> TokenStream {
    match result {
        Ok(stream) => stream,
        Err(error) => error.into_compile_error(),
    }
}

fn escape_member(member: Member) -> Ident {
    match member {
        Member::Named(ident) => ident,
        Member::Unnamed(index) => Ident::new(&format!("__self_{}", index.index), index.span),
    }
}

fn alloc() -> TokenStream {
    if cfg!(feature = "std") {
        quote! { ::std:: }
    } else {
        quote! { ::alloc:: }
    }
}
