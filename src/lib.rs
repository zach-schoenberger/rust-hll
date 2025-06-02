use dense::DenseRegisters;
use explicit::ExplicitStorage;
use settings::{Settings, SettingsError};
use sparse::SparseRegisters;
use thiserror::Error;

mod dense;
#[cfg(test)]
mod dense_test;
mod explicit;
#[cfg(test)]
mod integration_test;
mod settings;
mod sparse;
#[cfg(test)]
mod sparse_test;
mod utils;

/// `Register` is an add-on interface to storage that is implemented by the probabalistic types.
trait Registers {
    fn log_2m(&self) -> u32;
    fn pw_max_mask(&self) -> u64;
    fn m_bits_mask(&self) -> u64;

    /// set_if_greater sets the register value of register reg_num to the provided value if and only if it's greater than
    /// the current value.
    fn set_if_greater(&mut self, reg_num: u32, value: u8);

    /// indicator computes the "indicator function" (Z in the HLL paper).  It additionally returns the number of
    /// registers whose value is zero (V in the paper).  The returned values are used to drive cardinality calculations.
    ///
    /// For reference, Z = indicator(2^(-M[j])) for all j from 0 -> num registers where M[j] is the register value.
    fn indicator(&self) -> (f64, u32);

    /// calculates the register and value to use when calling `set_if_greater`. Returns None if
    /// value is 0.
    fn set(&mut self, value: u64) {
        // following documentation courtesy of the java implementation:
        //
        // p(w): position of the least significant set bit (one-indexed)
        // By contract: p(w) <= 2^(registerValueInBits) - 1 (the max register
        // value)
        //
        // By construction of pw_max_mask,
        //      lsb(pw_max_mask) = 2^(registerValueInBits) - 2,
        // thus lsb(any_long | pw_max_mask) <= 2^(registerValueInBits) - 2,
        // thus 1 + lsb(any_long | pw_max_mask) <= 2^(registerValueInBits) -1.
        let substream_value = value >> self.log_2m();
        if substream_value == 0 {
            // The paper does not cover p(0x0), so the special value 0 is used.
            // 0 is the original initialization value of the registers, so by
            // doing this the multiset simply ignores it. This is acceptable
            // because the probability is 1/(2^(2^registerSizeInBits)).
            return;
        }

        // NOTE : trailing zeros == the 0-based index of the least significant 1
        //        bit.
        let p_w = (1 + (substream_value | self.pw_max_mask()).trailing_zeros()) as u8;
        // NOTE:  no +1 as in paper since 0-based indexing
        let i = value & self.m_bits_mask();

        // this is safe because the m_bits_mask is 1 less bit in length than log_2m bits
        self.set_if_greater(i as u32, p_w);
    }
}

pub trait Storage {
    fn bytes_size(&self) -> usize;
    fn to_bytes(&self, buf: &mut [u8]);
    fn from_bytes(settings: &Settings, buf: &[u8]) -> Self;
    fn clear(&mut self);
}

#[derive(Clone, Debug, Error)]
pub enum HllError {
    #[error("{0}")]
    Settings(#[from] SettingsError),
    #[error("invalid version {0}")]
    Version(u8),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hll {
    Empty(Settings),
    Explicit(ExplicitStorage),
    Sparse(SparseRegisters),
    Dense(DenseRegisters),
}

impl Hll {
    pub fn new(settings: Settings) -> Self {
        Hll::Empty(settings)
    }

    pub(crate) fn add_raw(&mut self, value: u64) {
        if value == 0 {
            return;
        }

        if let Hll::Empty(settings) = self {
            if settings.explicit_threshold() > 0 {
                *self = Hll::Explicit(ExplicitStorage::with_settings(settings));
            } else if settings.sparse_threshold.is_some() {
                let registers = SparseRegisters::with_settings(settings);
                *self = Hll::Sparse(registers);
            } else {
                *self = Hll::Dense(DenseRegisters::with_settings(settings));
            }
        }

        match self {
            Hll::Explicit(explicit_registers) => {
                explicit_registers.set(value);
                if explicit_registers.is_full() {
                    *self = explicit_registers.as_registers();
                }
            }
            Hll::Sparse(sparse_registers) => {
                sparse_registers.set(value);

                if sparse_registers.is_full() {
                    *self = Hll::Dense(sparse_registers.to_dense(None));
                }
            }
            Hll::Dense(dense_registers) => {
                dense_registers.set(value);
            }
            _ => {}
        }
    }

    pub fn union(&mut self, strict: bool, other: &Self) -> Result<(), HllError> {
        if strict {
            self.settings_check(other)?;
        }

        match self {
            Hll::Empty(settings) => {
                *self = match &other {
                    Hll::Sparse(sparse_registers) => match settings.sparse_threshold {
                        Some(sparse_threshold) => {
                            if sparse_threshold < sparse_registers.len() as i32 {
                                Hll::Dense(sparse_registers.to_dense(Some(settings)))
                            } else {
                                Hll::Sparse(sparse_registers.clone())
                            }
                        }
                        None => Hll::Dense(sparse_registers.to_dense(Some(settings))),
                    },
                    _ => other.clone(),
                };
            }
            Hll::Explicit(lhs) => match other {
                Hll::Empty(_settings) => {}
                Hll::Explicit(rhs) => {
                    lhs.union_explicit(rhs);
                }
                Hll::Sparse(_sparse_registers) => {
                    let mut new_storage = lhs.as_registers();
                    new_storage.union(strict, other)?;

                    *self = new_storage;
                }
                Hll::Dense(_dense_registers) => {
                    let mut new_storage = lhs.as_registers();
                    new_storage.union(strict, other)?;

                    *self = new_storage;
                }
            },
            Hll::Sparse(sparse_registers) => match other {
                Hll::Empty(_settings) => {}
                Hll::Explicit(explicit_storage) => {
                    sparse_registers.union_explicit(explicit_storage);
                }
                Hll::Sparse(rhs_sparse_registers) => {
                    sparse_registers.union_sparse(rhs_sparse_registers);
                }
                Hll::Dense(dense_registers) => {
                    let mut new_storage = sparse_registers.to_dense(None);
                    new_storage.union_dense(dense_registers);

                    *self = Hll::Dense(new_storage);
                }
            },
            Hll::Dense(dense_registers) => match other {
                Hll::Empty(_settings) => {}
                Hll::Explicit(explicit_storage) => {
                    dense_registers.union_explicit(explicit_storage);
                }
                Hll::Sparse(sparse_registers) => {
                    dense_registers.union_sparse(sparse_registers);
                }
                Hll::Dense(rhs_dense_registers) => {
                    dense_registers.union_dense(rhs_dense_registers);
                }
            },
        }

        if self.is_full() {
            self.upgrade();
        }

        Ok(())
    }

    pub fn cardinality(&self) -> u64 {
        let (sum, num_of_zeros) = match self {
            Hll::Empty(_) => return 0,
            Hll::Explicit(explicit_storage) => return explicit_storage.len(),
            Hll::Sparse(sparse_registers) => sparse_registers.indicator(),
            Hll::Dense(dense_registers) => dense_registers.indicator(),
        };

        let settings = self.settings();

        // apply the estimate and correction to the indicator function
        let estimator = settings.alpha_msquared / sum;

        if (num_of_zeros != 0) && (estimator < settings.small_estimator_cutoff) {
            // following documentation courtesy of the java implementation:
            // The "small range correction" formula from the HyperLogLog
            // algorithm. Only appropriate if both the estimator is smaller than
            // (5/2) * m and there are still registers that have the zero value.
            let num_of_zeros = num_of_zeros as f64;
            let m: f64 = (1 << settings.log_2m).into();
            let small_estimator = m * (m / num_of_zeros).ln();
            return small_estimator.ceil() as u64;
        }

        if estimator <= settings.large_estimator_cutoff {
            return estimator.ceil() as u64;
        }

        // following documentation courtesy of the java implementation:
        // The "large range correction" formula from the HyperLogLog algorithm,
        // adapted for 64 bit hashes. Only appropriate for estimators whose
        // value exceeds the calculated cutoff.
        let large_estimator =
            -1.0 * settings.two_to_l * (1.0 - (estimator / settings.two_to_l)).ln();
        large_estimator.ceil() as u64
    }

    fn is_full(&self) -> bool {
        match self {
            Hll::Empty(_) => false,
            Hll::Explicit(explicit_storage) => explicit_storage.is_full(),
            Hll::Sparse(sparse_registers) => sparse_registers.is_full(),
            Hll::Dense(_) => false,
        }
    }

    fn upgrade(&mut self) {
        match self {
            Hll::Empty(_) => {}
            Hll::Explicit(explicit_storage) => {
                *self = explicit_storage.as_registers();
            }
            Hll::Sparse(sparse_registers) => {
                *self = Hll::Dense(sparse_registers.to_dense(None));
            }
            Hll::Dense(_) => {}
        }
    }

    pub fn settings_check(&self, other: &Self) -> Result<(), SettingsError> {
        self.settings().settings_check(other.settings())
    }

    pub fn settings(&self) -> &Settings {
        match self {
            Hll::Empty(settings) => settings,
            Hll::Explicit(explicit_storage) => &explicit_storage.settings,
            Hll::Sparse(sparse_registers) => &sparse_registers.settings,
            Hll::Dense(dense_registers) => &dense_registers.settings,
        }
    }

    pub fn clone_with_settings(&self, settings: &Settings) -> Self {
        match self {
            Hll::Empty(_) => Hll::Empty(*settings),
            Hll::Explicit(explicit_storage) => {
                Hll::Explicit(explicit_storage.clone_with_settings(settings))
            }
            Hll::Sparse(sparse_registers) => {
                Hll::Sparse(sparse_registers.clone_with_settings(settings))
            }
            Hll::Dense(dense_registers) => {
                Hll::Dense(dense_registers.clone_with_settings(settings))
            }
        }
    }

    pub fn type_id(&self) -> u8 {
        match self {
            Hll::Empty(_) => 1,
            Hll::Explicit(_) => 2,
            Hll::Sparse(_) => 3,
            Hll::Dense(_) => 4,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let (settings, type_id, size) = match self {
            Hll::Empty(settings) => (settings, 1, 0),
            Hll::Explicit(explicit_storage) => {
                (&explicit_storage.settings, 2, explicit_storage.bytes_size())
            }
            Hll::Sparse(sparse_registers) => {
                (&sparse_registers.settings, 3, sparse_registers.bytes_size())
            }
            Hll::Dense(dense_registers) => {
                (&dense_registers.settings, 4, dense_registers.bytes_size())
            }
        };
        let mut res: Vec<u8> = vec![0; 3 + size];

        res[0] = (1 << 4) | type_id;
        res[1] = (((settings.reg_width - 1) << 5) | settings.log_2m) as u8;
        res[2] = settings.pack_cutoff_byte();

        match self {
            Hll::Empty(_settings) => {}
            Hll::Explicit(explicit_storage) => {
                explicit_storage.to_bytes(&mut res[3..]);
            }
            Hll::Sparse(sparse_registers) => {
                sparse_registers.to_bytes(&mut res[3..]);
            }
            Hll::Dense(dense_registers) => {
                dense_registers.to_bytes(&mut res[3..]);
            }
        }

        res
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Self, HllError> {
        let version = buf[0] >> 4;
        let type_id = buf[0] & 0x0F;

        if version != 1 {
            return Err(HllError::Version(version));
        }

        let reg_width = (buf[1] >> 5) + 1;
        let log_2m = buf[1] & 0x1F;
        let (sparse_enabled, explicit_threshold) = Settings::unpack_cutoff_byte(buf[2]);

        let settings = Settings::new(
            log_2m as u32,
            reg_width as u32,
            explicit_threshold,
            sparse_enabled,
        )?;

        let storage = match type_id {
            1 => Self::Empty(settings),
            2 => Self::Explicit(ExplicitStorage::from_bytes(&settings, &buf[3..])),
            3 => Self::Sparse(SparseRegisters::from_bytes(&settings, &buf[3..])),
            4 => Self::Dense(DenseRegisters::from_bytes(&settings, &buf[3..])),
            _ => {
                return Err(HllError::Version(type_id));
            }
        };

        Ok(storage)
    }

    pub fn clear(&mut self) {
        match self {
            Hll::Empty(_) => {}
            Hll::Explicit(explicit_storage) => explicit_storage.clear(),
            Hll::Sparse(sparse_registers) => sparse_registers.clear(),
            Hll::Dense(dense_registers) => dense_registers.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hll() {
        // Create settings for the HLL
        let settings = Settings::new(
            10,   // log_2m: number of registers will be 2^10
            4,    // reg_width: 4 bits per register
            -1,   // explicit_threshold: auto-calculate threshold
            true, // sparse_enabled: use sparse representation
        )
        .unwrap();

        // Create a new HLL with the settings
        let mut hll = Hll::new(settings);

        // Add elements
        hll.add_raw(123456789);
        println!("Cardinality: {}", hll.cardinality()); // prints "1"

        // Create another HLL and add elements
        let mut hll2 = Hll::new(settings);
        hll2.add_raw(123456789);
        hll2.add_raw(987654321);

        // Union HLLs
        hll2.union(true, &hll).unwrap();
        println!("Cardinality after union: {}", hll2.cardinality()); // prints "2"

        // Serialize to bytes
        let bytes = hll2.to_bytes();

        // Deserialize from bytes
        let hll3 = Hll::from_bytes(&bytes).unwrap();
        println!("Cardinality after deserialization: {}", hll3.cardinality()); // prints "2"
    }
}
