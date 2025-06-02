use std::collections::BTreeMap;
use std::collections::btree_map::{Entry, Iter};

use crate::dense::DenseRegisters;
use crate::explicit::ExplicitStorage;
use crate::settings::Settings;
use crate::utils::{calc_position, divide_by_8_round_up, read_bits, write_bits};
use crate::{Registers, Storage};

#[derive(Clone, Debug, PartialEq)]
pub struct SparseRegisters {
    pub(crate) settings: Settings,
    pub(crate) buf: BTreeMap<u32, u8>,
}

impl SparseRegisters {
    pub fn with_settings(settings: &Settings) -> Self {
        Self {
            settings: *settings,
            buf: Default::default(),
        }
    }

    pub fn clone_with_settings(&self, settings: &Settings) -> Self {
        Self {
            settings: *settings,
            buf: self.buf.clone(),
        }
    }

    pub fn is_full(&self) -> bool {
        match self.settings.sparse_threshold {
            Some(threshold) => threshold < self.buf.len() as i32,
            None => true,
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn to_dense(&self, settings: Option<&Settings>) -> DenseRegisters {
        let mut registers = DenseRegisters::with_settings(settings.unwrap_or(&self.settings));
        for (reg_num, value) in self.buf.iter() {
            registers.set_reg(*reg_num, *value);
        }

        registers
    }

    pub fn union_sparse(&mut self, other: &Self) {
        for (reg_num, value) in other.buf.iter() {
            self.set_if_greater(*reg_num, *value);
        }
    }

    pub fn union_explicit(&mut self, explicit_storage: &ExplicitStorage) {
        for i in explicit_storage.iter() {
            self.set(i);
        }
    }

    pub fn iter(&self) -> Iter<'_, u32, u8> {
        self.buf.iter()
    }
}

impl Registers for SparseRegisters {
    fn set_if_greater(&mut self, reg_num: u32, value: u8) {
        let value = value & (self.settings.m_bits_mask as u8);

        match self.buf.entry(reg_num) {
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
            Entry::Occupied(mut occupied_entry) => {
                if *occupied_entry.get() < value {
                    occupied_entry.insert(value);
                }
            }
        };
    }

    fn indicator(&self) -> (f64, u32) {
        let mut sum: f64 = 0.0;

        for (_, v) in self.buf.iter() {
            sum += 1.0 / ((1_u64 << v) as f64)
        }

        let number_of_zeros = ((1 << self.settings.log_2m) - self.buf.len()) as u32;
        sum += number_of_zeros as f64;

        (sum, number_of_zeros)
    }

    fn log_2m(&self) -> u32 {
        self.settings.log_2m
    }

    fn pw_max_mask(&self) -> u64 {
        self.settings.pw_max_mask
    }

    fn m_bits_mask(&self) -> u64 {
        self.settings.m_bits_mask
    }
}

impl Storage for SparseRegisters {
    fn bytes_size(&self) -> usize {
        divide_by_8_round_up(
            (self.settings.log_2m + self.settings.reg_width)
                * TryInto::<u32>::try_into(self.buf.len()).unwrap(),
        )
        .try_into()
        .unwrap()
    }

    fn to_bytes(&self, buf: &mut [u8]) {
        let bits_per_register: u8 = (self.settings.log_2m + self.settings.reg_width)
            .try_into()
            .unwrap();

        for (i, (reg_num, reg)) in self.buf.iter().enumerate() {
            let (idx, pos) = calc_position(i as u32, bits_per_register);
            let reg: u32 = *reg as u32;
            write_bits(
                buf,
                idx,
                pos,
                (*reg_num << self.settings.reg_width) | reg,
                bits_per_register,
            );
        }
    }

    fn from_bytes(settings: &Settings, buf: &[u8]) -> Self {
        let bits_per_register = (settings.log_2m + settings.reg_width) as u8;
        let mut offset: u32 = 0;
        let max_offset = buf.len() as u32 * u8::BITS;
        let reg_mask: u32 = (1 << settings.reg_width) - 1;
        let reg_num_mask: u32 = !reg_mask;

        let mut res = Self::with_settings(settings);
        let mut i = 0;
        while (offset + bits_per_register as u32) <= max_offset {
            let (idx, pos) = calc_position(i, bits_per_register);
            let value = read_bits(buf, idx, pos, bits_per_register);
            let reg_num = (value & reg_num_mask) >> settings.reg_width;
            let reg_value = value & reg_mask;

            res.buf.insert(reg_num, reg_value as u8);

            offset += bits_per_register as u32;
            i += 1;
        }

        res
    }

    fn clear(&mut self) {
        self.buf.clear();
    }
}
