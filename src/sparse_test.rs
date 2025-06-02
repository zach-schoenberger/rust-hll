use crate::settings::Settings;
use crate::sparse::SparseRegisters;
use crate::{Registers, Storage};
use std::collections::BTreeMap;

fn sparse_test_settings() -> Settings {
    Settings::new(11, 5, 0, true).unwrap()
}

pub(crate) fn construct_hll_value(log2m: u32, register: u32, value: u32) -> u64 {
    let substream_value: u64 = 1 << (value - 1);
    let substream_value = substream_value << log2m;

    substream_value | register as u64
}

fn assert_register_present(registers: &SparseRegisters, register: u32, value: u8) {
    assert_eq!(registers.buf.get(&register), Some(&value));
}

fn assert_one_register_set(registers: &SparseRegisters, register: u32, value: u8) {
    assert_eq!(registers.buf.len(), 1);
    assert_register_present(registers, register, value);
}

fn assert_elements_equal_sparse(registers1: &SparseRegisters, registers2: &SparseRegisters) {
    assert_eq!(registers1.buf, registers2.buf);
}

#[test]
fn test_add_sparse() {
    // Test inserting an element with register value 1 (minimum set value)
    {
        let register_index = 0;
        let register_value = 1;
        let raw_value = construct_hll_value(
            sparse_test_settings().log_2m,
            register_index,
            register_value,
        );

        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        registers.set(raw_value);

        assert_one_register_set(&registers, register_index, register_value as u8);
    }

    // Test inserting an element with register value 31 (maximum set value)
    {
        let register_index = 0;
        let register_value = 31;
        let raw_value = construct_hll_value(
            sparse_test_settings().log_2m,
            register_index,
            register_value,
        );

        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        registers.set(raw_value);

        assert_one_register_set(&registers, register_index, register_value as u8);
    }

    // Test inserting an element that could overflow the register (past 31)
    {
        let register_index = 0;
        let register_value = 36;
        let raw_value = construct_hll_value(
            sparse_test_settings().log_2m,
            register_index,
            register_value,
        );

        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        registers.set(raw_value);

        assert_one_register_set(&registers, register_index, 31); // register max
    }

    // Test inserting duplicate elements, observe no change
    {
        let register_index = 0;
        let register_value = 1;
        let raw_value = construct_hll_value(
            sparse_test_settings().log_2m,
            register_index,
            register_value,
        );

        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        registers.set(raw_value);
        registers.set(raw_value);

        assert_one_register_set(&registers, register_index, register_value as u8);
    }

    // Test inserting elements that increase a register's value
    {
        let register_index = 0;
        let register_value = 1;
        let raw_value = construct_hll_value(
            sparse_test_settings().log_2m,
            register_index,
            register_value,
        );

        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        registers.set(raw_value);

        let register_value2 = 2;
        let raw_value2 = construct_hll_value(
            sparse_test_settings().log_2m,
            register_index,
            register_value2,
        );
        registers.set(raw_value2);

        assert_one_register_set(&registers, register_index, register_value2 as u8);
    }

    // Test inserting elements that have lower register values, observe no change
    {
        let register_index = 0;
        let register_value = 2;
        let raw_value = construct_hll_value(
            sparse_test_settings().log_2m,
            register_index,
            register_value,
        );

        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        registers.set(raw_value);

        let register_value2 = 1;
        registers.set_if_greater(register_index, register_value2 as u8);

        assert_one_register_set(&registers, register_index, register_value as u8);
    }
}

#[test]
fn test_union_sparse() {
    // Test two disjoint multisets should union properly
    {
        let mut registers_a = SparseRegisters::with_settings(&sparse_test_settings());
        registers_a.set_if_greater(1, 1);
        let mut registers_b = SparseRegisters::with_settings(&sparse_test_settings());
        registers_b.set_if_greater(2, 1);

        registers_a.union_sparse(&registers_b);

        assert_eq!(registers_a.buf.len(), 2);
        assert_register_present(&registers_a, 1, 1);
        assert_register_present(&registers_a, 2, 1);
    }

    // Test two exactly overlapping multisets should union properly
    {
        let mut registers_a = SparseRegisters::with_settings(&sparse_test_settings());
        registers_a.set_if_greater(1, 10);
        let mut registers_b = SparseRegisters::with_settings(&sparse_test_settings());
        registers_b.set_if_greater(1, 13);

        registers_a.union_sparse(&registers_b);

        assert_eq!(registers_a.buf.len(), 1);
        assert_one_register_set(&registers_a, 1, 13);
    }

    // Test overlapping multisets should union properly
    {
        let mut registers_a = SparseRegisters::with_settings(&sparse_test_settings());
        let mut registers_b = SparseRegisters::with_settings(&sparse_test_settings());

        // register index = 3
        registers_a.set_if_greater(3, 11);

        // register index = 4
        registers_a.set_if_greater(4, 13);
        registers_b.set_if_greater(4, 21);

        // register index = 5
        registers_b.set_if_greater(5, 14);

        registers_a.union_sparse(&registers_b);

        // union should have three registers set, with partition B set to the max of the two registers
        assert_register_present(&registers_a, 3, 11);
        assert_register_present(&registers_a, 4, 21); // max(21,13)
        assert_register_present(&registers_a, 5, 14);
    }
}

#[test]
fn test_to_from_bytes_sparse() {
    // Test empty element
    {
        let registers = SparseRegisters::with_settings(&sparse_test_settings());
        let mut buf = vec![0u8; registers.bytes_size()];
        registers.to_bytes(&mut buf);

        let from_bytes = SparseRegisters::from_bytes(&sparse_test_settings(), &buf);
        assert_eq!(from_bytes.buf.len(), 0);
    }

    // Test partially filled element
    {
        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        for i in 0..3 {
            registers.set_if_greater(i, (i + 9) as u8);
        }

        let mut buf = vec![0u8; registers.bytes_size()];
        registers.to_bytes(&mut buf);

        let from_bytes = SparseRegisters::from_bytes(&sparse_test_settings(), &buf);
        assert_elements_equal_sparse(&registers, &from_bytes);
    }

    // Test full set
    {
        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        for i in 0..sparse_test_settings().sparse_threshold.unwrap() as u32 {
            registers.set_if_greater(i, ((i % 9) + 1) as u8);
        }

        let mut buf = vec![0u8; registers.bytes_size()];
        registers.to_bytes(&mut buf);

        let from_bytes = SparseRegisters::from_bytes(&sparse_test_settings(), &buf);
        assert_elements_equal_sparse(&registers, &from_bytes);
    }
}

#[test]
fn test_random_values_sparse() {
    use rand::prelude::*;
    use rand::rngs::StdRng;

    let seed = 1; // makes for reproducible tests
    let mut rng = StdRng::seed_from_u64(seed);

    for _ in 0..100 {
        let mut registers = SparseRegisters::with_settings(&sparse_test_settings());
        let mut expected: BTreeMap<u32, u8> = BTreeMap::new();

        // Generate random values and add them to both the registers and expected map
        for _ in 0..100 {
            let register = rng.gen_range(0..(1 << sparse_test_settings().log_2m));
            let value: u8 = rng.gen_range(1..=31);
            registers.set_if_greater(register, value);

            // Update expected map with max value
            expected
                .entry(register)
                .and_modify(|e| *e = (*e).max(value))
                .or_insert(value);
        }

        // Verify all values match
        assert_eq!(registers.buf, expected);
    }
}
