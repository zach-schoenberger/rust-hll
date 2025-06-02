use crate::{
    Registers, Settings, Storage,
    explicit::ExplicitStorage,
    sparse::SparseRegisters,
    utils::{calc_position, divide_by_8_round_up, read_u8_bits, write_u8_bits},
};

#[derive(Debug, Clone, PartialEq)]
pub struct DenseRegisters {
    pub settings: Settings,
    buf: Vec<u8>,
}

impl DenseRegisters {
    pub fn with_settings(settings: &Settings) -> Self {
        let cap = divide_by_8_round_up((1 << settings.log_2m) * settings.reg_width);

        Self {
            settings: *settings,
            buf: vec![0; cap as usize],
        }
    }

    pub fn clone_with_settings(&self, settings: &Settings) -> Self {
        Self {
            settings: *settings,
            buf: self.buf.clone(),
        }
    }

    pub fn union_explicit(&mut self, explicit_storage: &ExplicitStorage) {
        for i in explicit_storage.iter() {
            self.set(i);
        }
    }

    pub fn union_sparse(&mut self, sparse_storage: &SparseRegisters) {
        for (reg_num, value) in sparse_storage.iter() {
            self.set_if_greater(*reg_num, *value);
        }
    }

    pub fn union_dense(&mut self, other: &Self) {
        for (reg_num, value) in other.iter() {
            self.set_if_greater(reg_num, value);
        }
    }

    // get extracts a single register value.  It is provided to enable union-ing two
    // dense storage instance with different Hll settings.
    pub fn get(&self, reg_num: u32) -> u8 {
        let (idx, pos) = calc_position(reg_num, self.settings.reg_width as u8);
        read_u8_bits(&self.buf, idx, pos, self.settings.reg_width as u8)
    }

    pub fn set_reg(&mut self, reg_num: u32, value: u8) {
        let (idx, pos) = calc_position(reg_num, self.settings.reg_width as u8);
        write_u8_bits(
            &mut self.buf,
            idx,
            pos,
            value,
            self.settings.reg_width as u8,
        );
    }

    pub fn iter(&self) -> RegisterIter {
        RegisterIter {
            reg_num: 0,
            reg_cnt: 1 << self.settings.log_2m,
            registers: self,
        }
    }
}

impl Registers for DenseRegisters {
    fn set_if_greater(&mut self, reg_num: u32, value: u8) {
        let (idx, pos) = calc_position(reg_num, self.settings.reg_width as u8);
        let register = read_u8_bits(&self.buf, idx, pos, self.settings.reg_width as u8);

        if value > register {
            write_u8_bits(
                &mut self.buf,
                idx,
                pos,
                value,
                self.settings.reg_width as u8,
            );
        }
    }

    fn indicator(&self) -> (f64, u32) {
        let mut sum: f64 = 0.0;
        let mut number_of_zeros = 0;
        let num_reg = 1 << self.settings.log_2m;

        for i in 0..num_reg {
            let value = self.get(i);

            sum += 1.0 / ((1_u64 << value) as f64);
            if value == 0 {
                number_of_zeros += 1;
            }
        }

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

impl Storage for DenseRegisters {
    fn bytes_size(&self) -> usize {
        self.buf.len()
    }

    fn to_bytes(&self, buf: &mut [u8]) {
        assert!(buf.len() >= self.buf.len());
        buf[..self.buf.len()].copy_from_slice(&self.buf);
    }

    fn from_bytes(settings: &Settings, buf: &[u8]) -> Self {
        let mut res = Self::with_settings(settings);
        assert!(res.buf.len() >= buf.len());
        res.buf[..buf.len()].copy_from_slice(buf);

        res
    }

    fn clear(&mut self) {
        for i in 0..self.buf.len() {
            self.buf[i] = 0;
        }
    }
}

pub struct RegisterIter<'a> {
    reg_num: u32,
    reg_cnt: u32,
    registers: &'a DenseRegisters,
}

impl Iterator for RegisterIter<'_> {
    type Item = (u32, u8);

    fn next(&mut self) -> Option<Self::Item> {
        if self.reg_num < self.reg_cnt {
            let value = self.registers.get(self.reg_num);
            let res = Some((self.reg_num, value));
            self.reg_num += 1;

            return res;
        }

        None
    }
}
