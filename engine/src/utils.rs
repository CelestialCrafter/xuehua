use std::{fs, io, path::Path};

use mlua::{Function, Lua, chunk};

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

        impl<T, E: Into<Box<dyn std::error::Error + Send + Sync>>> ExternalResult<T> for Result<T, E>
        {
            $(fn $fn(self) -> Result<T, $error> {
                self.map_err(|err| err.$fn())
            })*
        }

        impl<E: Into<Box<dyn std::error::Error + Send + Sync>>> ExternalError for E
        {
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
