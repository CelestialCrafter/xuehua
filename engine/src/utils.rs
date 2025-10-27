use std::{fs, io, path::Path};

use mlua::{Function, Lua, chunk};

pub type BoxDynError = Box<dyn std::error::Error + Send + Sync>;

#[macro_export]
macro_rules! impl_into_err {
    ($(($error:ty, $fn:ident)),*) => {
        /// Trait for converting [`std::result::Result`] into Lua [`Result`].
        pub trait ExternalResult<T> {
            $(fn $fn(self) -> Result<T, $error>;)*
        }

        pub trait ExternalError {
            $(fn $fn(self) -> $error;)*
        }

        impl<T, E: Into<crate::utils::BoxDynError>> ExternalResult<T> for Result<T, E> {
            $(fn $fn(self) -> Result<T, $error> {
                self.map_err(|err| err.$fn())
            })*
        }

        impl<E: Into<crate::utils::BoxDynError>> ExternalError for E {
            $(fn $fn(self) -> $error {
                <$error>::ExternalError(self.into())
            })*
        }
    };
}

pub mod passthru {
    use std::{
        collections::{HashMap, HashSet},
        hash::{BuildHasherDefault, Hasher},
    };

    #[derive(Default)]
    pub struct PassthruHasher(u64);

    impl Hasher for PassthruHasher {
        fn finish(&self) -> u64 {
            self.0
        }

        fn write_u64(&mut self, i: u64) {
            self.0 = i;
        }

        fn write_usize(&mut self, i: usize) {
            self.write_u64(i as u64);
        }

        fn write_u32(&mut self, i: u32) {
            self.write_u64(i as u64);
        }

        fn write_u16(&mut self, i: u16) {
            self.write_u64(i as u64);
        }

        fn write_u8(&mut self, i: u8) {
            self.write_u64(i as u64);
        }

        fn write(&mut self, _: &[u8]) {
            unimplemented!("passthru does not support Hasher::write()")
        }
    }

    pub type PassthruHashMap<K, V> = HashMap<K, V, BuildHasherDefault<PassthruHasher>>;
    pub type PassthruHashSet<T> = HashSet<T, BuildHasherDefault<PassthruHasher>>;
}

pub mod scope {
    use frunk_core::hlist::{HCons, HNil};
    use mlua::{AnyUserData, Error, Function, Lua, Table, UserData};
    use std::marker::PhantomData;

    pub trait LuaScopeRelease: Sized {
        fn release_inner(data: &mut Vec<AnyUserData>) -> Result<Self, Error>;
    }

    pub struct LuaScope<'a, T> {
        lua: &'a Lua,
        environment: Table,
        data: Vec<AnyUserData>,
        _marker: PhantomData<T>,
    }

    impl<'a> LuaScope<'a, HNil> {
        pub fn from_function(lua: &'a Lua, function: &Function) -> Result<Self, Error> {
            let environment = function.environment().ok_or(Error::external(
                "LuaScope::from_function can't be used with rust/c functions",
            ))?;
            Ok(Self::new(lua, environment))
        }

        pub fn new(lua: &'a Lua, environment: Table) -> Self {
            Self {
                lua,
                environment,
                data: Vec::new(),
                _marker: PhantomData,
            }
        }
    }

    impl<'a, T> LuaScope<'a, T> {
        pub fn push_data<H: UserData + Send + 'static>(
            mut self,
            key: &str,
            data: H,
        ) -> Result<LuaScope<'a, HCons<H, T>>, Error> {
            let userdatum = self.lua.create_userdata(data)?;
            self.environment.set(key, &userdatum)?;
            self.data.push(userdatum);

            Ok(LuaScope::<'_, HCons<H, T>> {
                lua: self.lua,
                environment: self.environment,
                data: self.data,
                _marker: PhantomData,
            })
        }
    }

    impl<T: LuaScopeRelease> LuaScope<'_, T> {
        pub fn release(mut self) -> Result<T, Error> {
            T::release_inner(&mut self.data)
        }
    }

    impl LuaScopeRelease for HNil {
        fn release_inner(_data: &mut Vec<AnyUserData>) -> Result<Self, Error> {
            Ok(HNil)
        }
    }

    impl<H: 'static, T: LuaScopeRelease> LuaScopeRelease for HCons<H, T> {
        fn release_inner(data: &mut Vec<AnyUserData>) -> Result<Self, Error> {
            let datum = data
                .pop()
                .expect("not enough userdata in scope for type constraints")
                .take()?;

            Ok(HCons {
                head: datum,
                tail: T::release_inner(data)?,
            })
        }
    }
}

pub fn ensure_dir(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(_) => Ok(()),
        Err(_) if path.is_dir() => Ok(()),
        Err(err) => Err(err),
    }
}

pub fn register_module(lua: &Lua) -> Result<(), mlua::Error> {
    let module = lua.create_table()?;

    let [runtime, buildtime, no_config] = lua
        .load(chunk! {
            local function runtime(pkg)
                return { type = "runtime", package = pkg }
            end

            local function buildtime(pkg)
                return { type = "buildtime", package = pkg }
            end

            local function no_config(pkg)
                pkg.defaults = {}
                pkg.configure = function(_)
                    return pkg
                end

                return pkg
            end

            return { runtime, buildtime, no_config }
        })
        .eval::<[Function; 3]>()
        .expect("util functions should evaluate");

    module.set("runtime", runtime)?;
    module.set("buildtime", buildtime)?;
    module.set("no_config", no_config)?;

    lua.register_module("xuehua.utils", module)?;

    Ok(())
}
