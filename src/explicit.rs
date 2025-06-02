use std::collections::BTreeSet;

use crate::{Hll, Storage, dense::DenseRegisters, settings::Settings, sparse::SparseRegisters};

#[derive(Debug, Clone, PartialEq)]
pub struct ExplicitStorage {
    pub settings: Settings,
    buf: BTreeSet<i64>,
}

impl ExplicitStorage {
    pub fn with_settings(settings: &Settings) -> Self {
        Self {
            settings: *settings,
            buf: BTreeSet::new(),
        }
    }

    pub fn clone_with_settings(&self, settings: &Settings) -> Self {
        Self {
            settings: *settings,
            buf: self.buf.clone(),
        }
    }

    pub fn as_registers(&self) -> Hll {
        let mut storage = match self.settings.sparse_threshold {
            Some(_) => Hll::Sparse(SparseRegisters::with_settings(&self.settings)),
            None => Hll::Dense(DenseRegisters::with_settings(&self.settings)),
        };

        for value in self.buf.iter() {
            storage.add_raw(*value as u64);
        }

        storage
    }

    pub fn set(&mut self, value: u64) {
        self.buf.insert(value as i64);
    }

    pub fn is_full(&self) -> bool {
        self.buf.len() as u32 > self.settings.explicit_threshold()
    }

    pub fn union_explicit(&mut self, other: &Self) {
        self.buf.extend(other.buf.iter());
    }

    pub fn iter(&self) -> impl Iterator<Item = u64> {
        self.buf.iter().map(|i| *i as u64)
    }

    pub fn len(&self) -> u64 {
        self.buf.len() as u64
    }
}

impl Storage for ExplicitStorage {
    fn bytes_size(&self) -> usize {
        size_of::<i64>() * self.buf.len()
    }

    fn to_bytes(&self, buf: &mut [u8]) {
        for (i, value) in self.buf.iter().enumerate() {
            let idx = i * size_of::<i64>();
            buf[idx..(idx + size_of::<i64>())].copy_from_slice(&(*value).to_be_bytes());
        }
    }

    fn from_bytes(settings: &Settings, buf: &[u8]) -> Self {
        let mut res = Self::with_settings(settings);
        let mut idx = 0;

        while idx < buf.len() {
            let s = &buf[idx..(idx + size_of::<i64>())];
            let value = i64::from_be_bytes(s.try_into().unwrap());
            res.buf.insert(value);

            idx += size_of::<i64>();
        }

        res
    }

    fn clear(&mut self) {
        self.buf.clear();
    }
}
