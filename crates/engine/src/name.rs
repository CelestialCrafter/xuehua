use std::{fmt, str::FromStr, sync::Arc};

use smol_str::SmolStr;
use xh_reports::prelude::*;

#[derive(Default, IntoReport)]
#[message("could not parse name")]
pub struct ParseError;

#[derive(Default, Debug, Clone, Hash, PartialEq, Eq)]
pub struct Name<T: NameType> {
    pub identifier: SmolStr,
    pub namespace: Arc<[SmolStr]>,
    pub ty: T,
}

impl<T: NameType> Name<T> {
    #[inline]
    pub fn new() -> Self {
        Name {
            identifier: SmolStr::new_static(""),
            namespace: Arc::new([]),
            ty: T::default(),
        }
    }

    pub fn with_ident(mut self, ident: impl Into<SmolStr>) -> Self {
        self.identifier = ident.into();
        self
    }

    pub fn with_type<U: NameType>(self, ty: U) -> Name<U> {
        Name {
            identifier: self.identifier,
            namespace: self.namespace,
            ty,
        }
    }
}

impl<T: NameType> FromStr for Name<T> {
    type Err = Report<ParseError>;

    fn from_str(s: &str) -> Result<Self, ParseError> {
        let rest = s;
        let (rest, ty) = match rest.rsplit_once('(') {
            Some((rest, ty)) => {
                let ty = ty.strip_suffix(')').ok_or(ParseError)?;
                let ty = T::from_str(ty)?;
                (rest, ty)
            }
            None => (rest, T::default()),
        };

        let (identifier, namespace) = match rest.split_once("@") {
            Some((identifier, rest)) => {
                let namespace = rest.split('/').map(Into::into);
                (identifier.into(), namespace.collect())
            }
            None => (rest.into(), Arc::default()),
        };

        Ok(Self {
            identifier,
            namespace,
            ty,
        })
    }
}

#[macro_export]
macro_rules! gen_name {
    ($ident:ident @ $($namespace:ident) / *) => {
        $crate::name::Name {
            identifier: stringify!($ident).into(),
            namespace: [$(stringify!($namespace).into()),*].into(),
            ty: Default::default(),
        }
    };
}

pub trait NameType: Default + fmt::Display + FromStr<Err = ParseError> {}

macro_rules! impl_name_type {
    ($(($name:ident, $alias:ident, $type:expr)),*) => {$(
        #[derive(Default, Debug, Clone, Hash, PartialEq, Eq)]
        pub struct $name;

        pub type $alias = Name<$crate::name::$name>;

        impl FromStr for $name {
            type Err = ParseError;

            fn from_str(s: &str) -> StdResult<Self, Self::Err> {
               if s == $type {
                   Ok(Self)
               } else {
                   Err(ParseError)
               }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str($type)
            }
        }

        impl NameType for $name {}
    )*};
}

impl_name_type!((Package, PackageName, "package"));
impl_name_type!((Executor, ExecutorName, "executor"));
impl_name_type!((Backend, BackendName, "backend"));
impl_name_type!((Store, StoreName, "store"));

impl<T: NameType> fmt::Display for Name<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.namespace.is_empty() {
            self.identifier.fmt(f)
        } else {
            write!(
                f,
                "{}@{}({})",
                self.identifier,
                self.namespace.join("/"),
                T::default()
            )
        }
    }
}
