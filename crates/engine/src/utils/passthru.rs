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

    fn write_usize(&mut self, _: usize) {
        unimplemented!("passthru does not support Hasher::write_usize()")
    }

    fn write_u32(&mut self, i: u32) {
        self.write_u64(i.into());
    }

    fn write_u16(&mut self, i: u16) {
        self.write_u64(i.into());
    }

    fn write_u8(&mut self, i: u8) {
        self.write_u64(i.into());
    }

    fn write(&mut self, _: &[u8]) {
        unimplemented!("passthru does not support Hasher::write()")
    }
}

pub type PassthruHashMap<K, V> = HashMap<K, V, BuildHasherDefault<PassthruHasher>>;
pub type PassthruHashSet<T> = HashSet<T, BuildHasherDefault<PassthruHasher>>;
