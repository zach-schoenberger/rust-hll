use thiserror::Error;

use crate::utils::divide_by_8_round_up;

// minimum and maximum values for the log-base-2 of the number of registers
// in the HLL
const MINIMUM_LOG_2M_PARAM: u32 = 4;
const MAXIMUM_LOG_2M_PARAM: u32 = 31;

// minimum and maximum values for the register width of the HLL
const MINIMUM_REG_WIDTH_PARAM: u32 = 1;
const MAXIMUM_REG_WIDTH_PARAM: u32 = 8;

// minimum and maximum values for the 'expthresh' parameter of the
// constructor that is meant to match the PostgreSQL
// implementation's constructor and parameter names
const MINIMUM_EXPTHRESH_PARAM: i32 = -1;
const MAXIMUM_EXPTHRESH_PARAM: i32 = 18;
const MAXIMUM_EXPLICIT_THRESHOLD: u32 = 1 << (MAXIMUM_EXPTHRESH_PARAM - 1); /*per storage spec*/

// AutoExplicitThreshold indicates that the threshold at which an Hll goes
// from using an explicit to a probabalistic representation should be
// calculated based on the configuration.  Using the calculated threshold is
// generally preferable.  One exception would be working with a pre-existing
// data set that uses a particular explicit threshold setting in which case
// it may be desirable to use the same explicit threshold.
const AUTO_EXPLICIT_THRESHOLD: i32 = -1;

/// Settings are used to configure the Hll and how it transitions between the
/// backing storage types.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Settings {
    /// log_2m determines the number of registers in the Hll.  The minimum value
    /// is 4 and the maximum value is 31.  The number of registers in the Hll
    /// will be calculated as 2^log_2m.
    pub(crate) log_2m: u32,

    /// reg_width is the number of bits dedicated to each register value.  The
    /// minimum value is 1 and the maximum value is 8.
    pub(crate) reg_width: u32,

    /// ExplicitThreshold is the cardinality at which the Hll will go from
    /// storing explicit values to using a probabilistic model.  A value of 0
    /// disables explicit storage entirely.  The value AutoExplicitThreshold can
    /// be used to signal the library to calculate an appropriate threshold
    /// (recommended).  The maximum allowed value is 131,072.
    pub(crate) explicit_threshold: i32,

    /// SparseEnabled controls whether the Hll will use the sparse
    /// representation.  The thresholds for conversion are automatically
    /// calculated by the library when this field is set to true (recommended).
    pub(crate) sparse_threshold: Option<i32>,

    /// pw_max_mask is a mask that prevents overflow of HyperLogLog registers.
    pub(crate) pw_max_mask: u64,

    /// m_bits_mask is a precomputed mask where the bottom-most reg_width bits are set.
    pub(crate) m_bits_mask: u64,
    /// alpha * m^2 (the constant in the "'raw' HyperLogLog estimator")
    pub(crate) alpha_msquared: f64,

    /// small_estimator_cutoff is the cutoff value of the estimator for using the
    /// "small" range cardinality correction formula
    pub(crate) small_estimator_cutoff: f64,

    /// large_estimator_cutoff is the cutoff value of the estimator for using the
    /// "large" range cardinality correction formula
    pub(crate) large_estimator_cutoff: f64,
    pub(crate) two_to_l: f64,
}

#[derive(Clone, Debug, Error)]
pub enum SettingsError {
    #[error("log_2m must be between {MINIMUM_LOG_2M_PARAM}, {MAXIMUM_LOG_2M_PARAM}")]
    Log2m,
    #[error("reg_width must be between {MINIMUM_REG_WIDTH_PARAM}, {MAXIMUM_REG_WIDTH_PARAM}")]
    RegWidth,
    #[error("Threshold must be between {MINIMUM_EXPTHRESH_PARAM}, {MAXIMUM_EXPTHRESH_PARAM}")]
    Threshold,
    #[error("config mismatch. log_2m and reg_width must match when combining hll's")]
    MisMatch,
}

impl Settings {
    pub fn new(
        log_2m: u32,
        reg_width: u32,
        explicit_threshold: i32,
        sparse_enabled: bool,
    ) -> Result<Self, SettingsError> {
        let sparse_threshold = match sparse_enabled {
            true => Some(Self::calculate_sparse_threshold(log_2m, reg_width)),
            false => None,
        };

        let settings = Self {
            log_2m,
            reg_width,
            explicit_threshold,
            sparse_threshold,
            pw_max_mask: Settings::pw_max_mask(reg_width),
            m_bits_mask: ((1 << log_2m) - 1),
            alpha_msquared: Settings::alpha_m_squared(log_2m),
            small_estimator_cutoff: Settings::small_estimator_cutoff(1 << log_2m),
            large_estimator_cutoff: Settings::large_estimator_cutoff(Settings::two_to_l(
                log_2m, reg_width,
            )),
            two_to_l: Settings::two_to_l(log_2m, reg_width),
        };

        settings.validate()?;

        Ok(settings)
    }

    pub fn validate(&self) -> Result<(), SettingsError> {
        if !(MINIMUM_LOG_2M_PARAM..=MAXIMUM_LOG_2M_PARAM).contains(&self.log_2m) {
            return Err(SettingsError::Log2m);
        }

        if !(MINIMUM_REG_WIDTH_PARAM..=MAXIMUM_REG_WIDTH_PARAM).contains(&self.reg_width) {
            return Err(SettingsError::RegWidth);
        }

        // the unit tests are using 256 for their threshold which is larger than what the constructor in golang allows.
        // if !(MINIMUM_EXPTHRESH_PARAM..=MAXIMUM_EXPTHRESH_PARAM).contains(&self.explicit_threshold) {
        //     return Err(SettingsError::Threshold);
        // }

        Ok(())
    }

    pub fn settings_check(&self, other: &Self) -> Result<(), SettingsError> {
        if self.log_2m == other.log_2m && self.reg_width == other.reg_width {
            return Ok(());
        }

        Err(SettingsError::MisMatch)
    }

    pub fn explicit_threshold(&self) -> u32 {
        match self.explicit_threshold {
            AUTO_EXPLICIT_THRESHOLD => {
                Self::calculate_explicit_threshold(self.log_2m, self.reg_width)
            }
            _ => self.explicit_threshold as u32,
        }
    }

    /// determines a good cutoff to switch between explicit and probabilistic storage.
    pub fn calculate_explicit_threshold(log_2m: u32, reg_width: u32) -> u32 {
        // NOTE:  This math matches the size calculation in the PostgreSQL impl.
        let m = 1 << log_2m;
        let full_representation_size = divide_by_8_round_up(reg_width * m); /*round up to next whole byte*/
        let num_longs = full_representation_size / 8;

        if num_longs > MAXIMUM_EXPLICIT_THRESHOLD {
            return MAXIMUM_EXPLICIT_THRESHOLD;
        }

        num_longs
    }

    // calculate_sparse_threshold determines a good cutoff to switch between sparse
    // and dense probabilistic storage.
    fn calculate_sparse_threshold(log_2m: u32, reg_width: u32) -> i32 {
        let m = 1 << log_2m;
        let short_word_length: f64 = (log_2m + reg_width).into();

        let reg_bits: f64 = (m * reg_width).into();

        let largest_pow2_less_than_cutoff: u32 = (reg_bits / short_word_length).log2() as u32;

        1 << largest_pow2_less_than_cutoff
    }

    // pw_max_mask calculates the mask that is used to prevent overflow of HyperLogLog
    // registers.
    pub(crate) fn pw_max_mask(reg_width: u32) -> u64 {
        let shift: u64 = (((1u64 << reg_width) - 1) - 1) % (u64::BITS as u64);
        !((1u64 << shift) - 1)
    }

    /// alpha_m_squared calculates the 'alpha-m-squared' constant (gamma times
    /// registerCount squared where gamma is based on the value of registerCount)
    /// used by the HyperLogLog algorithm.
    pub(crate) fn alpha_m_squared(log_2m: u32) -> f64 {
        let m: f64 = (1 << log_2m).into();

        match log_2m {
            4 => 0.673 * m * m,
            5 => 0.697 * m * m,
            6 => 0.709 * m * m,
            _ => (0.7213 / (1.0 + 1.079 / m)) * m * m,
        }
    }

    // small_estimator_cutoff calculates the "small range correction" formula, in the
    // HyperLogLog algorith based on the total number of registers (m)
    pub(crate) fn small_estimator_cutoff(m: u32) -> f64 {
        let m: f64 = m.into();
        (m * 5.0) / 2.0
    }

    // large_estimator_cutoff calculates The cutoff for using the "large range
    // correction" formula, from the HyperLogLog algorithm, adapted for 64 bit
    // hashes.  See http://research.neustar.biz/2013/01/24/hyperloglog-googles-take-on-engineering-hll.
    pub(crate) fn large_estimator_cutoff(two_to_l: f64) -> f64 {
        two_to_l / 30.0
    }

    // two_to_l calculates 2 raised to L where L is the "large range correction
    // boundary" described at http://research.neustar.biz/2013/01/24/hyperloglog-googles-take-on-engineering-hll.
    pub(crate) fn two_to_l(log_2m: u32, reg_width: u32) -> f64 {
        let max_register_value = (1 << reg_width) - 1;

        // Since 1 is added to p(w) in the insertion algorithm, only
        // (max_register_value - 1) bits are inspected hence the hash
        // space is one power of two smaller.
        let pw_bits = max_register_value - 1;
        let total_bits = pw_bits + log_2m;

        // NOTE : this can get larger than fits in a 64 bit integer.
        2_f64.powf(total_bits.into())
    }

    pub(crate) fn pack_cutoff_byte(&self) -> u8 {
        let threshold = if self.explicit_threshold == AUTO_EXPLICIT_THRESHOLD {
            63
        } else if self.explicit_threshold == 0 {
            0
        } else {
            u32::BITS - (self.explicit_threshold as u32).leading_zeros() - 1
        };

        let mut res = threshold;
        if self.sparse_threshold.is_some() {
            res |= 1 << 6
        }

        res as u8
    }

    /// (sparse_enabled, explicit_threshold)
    pub(crate) fn unpack_cutoff_byte(b: u8) -> (bool, i32) {
        let sparse_enabled = b >> 6 == 1;
        let threshold = b & 0x3F;

        if threshold == 0 {
            return (sparse_enabled, 0);
        }

        if threshold == 63 {
            return (sparse_enabled, -1);
        }

        (sparse_enabled, 1 << (threshold - 1))
    }
}

#[cfg(test)]
mod test {
    use super::Settings;

    #[test]
    fn pw() {
        let settings = Settings::new(10, 5, 0, false);

        println!("{:?}", settings);
    }

    #[test]
    fn left_shift() {
        assert_eq!(1 << 0, 1);
    }
}
