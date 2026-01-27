#![no_std]

extern crate alloc;

use alloc::{format, string::ToString, vec::Vec};
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{
    Attribute, Data, DeriveInput, Error, Fields, Ident, LitStr, Member, Token, Variant,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
};

#[derive(Default, Debug, Clone, Copy)]
enum Mode {
    Debug,
    #[default]
    Display,
}

impl Mode {
    fn format(&self, value: impl ToTokens) -> TokenStream {
        match self {
            Mode::Debug => quote!(format_args!("{:?}", #value)),
            Mode::Display => quote!(#value),
        }
    }
}

impl Parse for Mode {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        mod kw {
            syn::custom_keyword!(display);
            syn::custom_keyword!(debug);
        }

        let lookahead = input.lookahead1();
        if lookahead.peek(kw::debug) {
            input.parse::<kw::debug>().map(|_| Mode::Debug)
        } else if lookahead.peek(kw::display) {
            input.parse::<kw::display>().map(|_| Mode::Display)
        } else {
            Err(lookahead.error())
        }
    }
}

#[proc_macro_derive(
    IntoReport,
    attributes(suggestion, attachment, context, message, format)
)]
pub fn derive_into_report(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let output = build_into_report_impl(&input);

    output.into()
}

fn build_into_report_impl(input: &DeriveInput) -> TokenStream {
    fn build_frames(attrs: &[Attribute], fields: &Fields) -> impl Iterator<Item = TokenStream> {
        attrs.iter().filter_map(|attr| {
            let path = attr.path();
            let ty = if path.is_ident("suggestion") {
                build_suggestion(fields, attr)
            } else if path.is_ident("attachment") {
                build_attachment(attr)
            } else if path.is_ident("context") {
                build_context(attr).map(|frames| quote! { #(#frames),* })
            } else {
                return None;
            };

            Some(flatten_result(ty))
        })
    }

    fn build_msg(ident: &Ident, attrs: &[Attribute], fields: &Fields) -> TokenStream {
        let msg = attrs
            .iter()
            .find(|attr| attr.path().is_ident("message"))
            .map(|attr| flatten_result(build_formatted(fields, "message", attr)));
        let msg =
            flatten_result(msg.ok_or_else(|| {
                Error::new(ident.span(), "type should have the #[message] attribute")
            }));

        quote!(#msg)
    }

    let data = match &input.data {
        Data::Struct(data) => {
            let msg = build_msg(&input.ident, &input.attrs, &data.fields);
            let frames = build_frames(&input.attrs, &data.fields);
            let bindings = build_bindings_struct(&data.fields);

            quote! {{
                #bindings
                (#msg, [#(#frames),*])
            }}
        }
        Data::Enum(data) => {
            let arms = data.variants.iter().map(|variant| {
                let msg = build_msg(&variant.ident, &variant.attrs, &variant.fields);
                let frames = build_frames(&variant.attrs, &variant.fields);
                let bindings = build_bindings_enum(variant);

                let enum_ident = &input.ident;
                let variant_ident = &variant.ident;

                let alloc = alloc();
                quote! { #enum_ident::#variant_ident #bindings => (#msg, #alloc::vec![#(#frames),*])}
            });

            quote! { match self { #(#arms),* } }
        }
        Data::Union(_) => unsupported_error(input.span(), "unions").to_compile_error(),
    };

    let ident = &input.ident;
    quote! {
        impl ::xh_reports::IntoReport for #ident {
            fn into_report(self) -> ::xh_reports::Report<Self> {
                let (msg, frames) = #data;
                ::xh_reports::Report::new(msg).with_frames(frames)
            }
        }
    }
}

fn build_bindings_enum(variant: &Variant) -> TokenStream {
    let members = variant.fields.members().map(escape_member);
    match &variant.fields {
        Fields::Named(_) => quote! { {#(ref #members),*} },
        Fields::Unnamed(_) => quote! { (#(ref #members),*) },
        Fields::Unit => TokenStream::new(),
    }
}

fn build_bindings_struct(fields: &Fields) -> TokenStream {
    fields
        .members()
        .map(|member| {
            let escaped = escape_member(member.clone());
            quote! { let #escaped  = &self.#member; }
        })
        .collect()
}

fn unsupported_error(span: Span, feature: impl core::fmt::Display) -> Error {
    Error::new(
        span,
        format_args!("#[derive(IntoReport)] does not support {feature}"),
    )
}

fn build_suggestion(fields: &Fields, attr: &Attribute) -> Result<TokenStream, Error> {
    let fmt = build_formatted(fields, "suggestion", attr)?;
    Ok(quote!(::xh_reports::Frame::suggestion(#fmt)))
}

fn build_attachment(attr: &Attribute) -> Result<TokenStream, Error> {
    let value = attr.parse_args_with(|input: ParseStream| {
        let mode = match Mode::parse(input) {
            Ok(mode) => {
                input.parse::<Token![:]>()?;
                mode
            }
            Err(_) => Default::default(),
        };

        let value = mode.format(Member::parse(input)?);
        Ok(value)
    })?;

    Ok(quote! ( ::xh_reports::Frame::attachment(#value) ))
}

fn build_context(attr: &Attribute) -> Result<impl Iterator<Item = TokenStream>, Error> {
    let (mode, members) = attr.parse_args_with(|stream: ParseStream| {
        let mode = match Mode::parse(stream) {
            Ok(mode) => {
                stream.parse::<Token![:]>()?;
                mode
            }
            Err(_) => Default::default(),
        };

        struct Mapping {
            source: Member,
            dest: Option<Member>,
        }

        impl Parse for Mapping {
            fn parse(input: ParseStream) -> syn::Result<Self> {
                Ok(Self {
                    source: Member::parse(input)?,
                    dest: if input.lookahead1().peek(Token![=]) {
                        input
                            .parse::<Token![=]>()
                            .and_then(|_| Member::parse(input))
                            .map(Some)?
                    } else {
                        None
                    },
                })
            }
        }

        let members = Punctuated::<Mapping, Token![,]>::parse_terminated(stream)?;
        Ok((mode, members))
    })?;

    let frames = members.into_iter().map(move |pair| {
        let (dest, span) = match pair.dest.as_ref().unwrap_or(&pair.source) {
            Member::Named(ident) => (ident.to_string(), ident.span()),
            Member::Unnamed(index) => (index.index.to_string(), index.span),
        };

        let key = LitStr::new(&dest, span);
        let value = mode.format(escape_member(pair.source));

        quote! { ::xh_reports::Frame::context(#key, #value) }
    });

    Ok(frames)
}

fn build_formatted(fields: &Fields, target: &str, attr: &Attribute) -> Result<TokenStream, Error> {
    let fmt: LitStr = attr.parse_args()?;
    let members: Vec<_> = fields
        .iter()
        .filter_map(|field| {
            field
                .attrs
                .iter()
                .filter(|attr| attr.path().is_ident("format"))
                .find_map(|attr| match attr.parse_args::<Ident>() {
                    Ok(ident) => (ident == target).then_some(Ok(field)),
                    Err(err) => Some(Err(err)),
                })
        })
        .enumerate()
        .map(|(i, result)| {
            result.map(|field| match field.ident {
                Some(ref ident) => ident.clone().into(),
                None => i.into(),
            })
        })
        .collect::<Result<_, _>>()?;

    let bindings = match fields {
        Fields::Named(_) => quote!(#(#members = #members),*),
        Fields::Unnamed(_) => {
            let members = members.into_iter().map(escape_member);
            quote!(#(#members),*)
        }
        Fields::Unit => TokenStream::new(),
    };

    let alloc = alloc();
    Ok(quote!({
        use #alloc::borrow::Cow;
        use #alloc::string::ToString;

        let fmt = format_args!(#fmt, #bindings);
        match fmt.as_str() {
            Some(string) => Cow::Borrowed(string),
            None => Cow::Owned(fmt.to_string())
        }
    }))
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
        quote! { ::std }
    } else {
        quote! { ::alloc }
    }
}
