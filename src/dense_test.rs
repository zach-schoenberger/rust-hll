use crate::{
    Hll, Registers, Settings, dense::DenseRegisters, sparse_test::construct_hll_value,
    utils::divide_by_8_round_up,
};

fn dense_test_settings() -> Settings {
    Settings::new(11, 5, 0, false).unwrap()
}

#[test]
fn test_clear_dense() {
    let mut hll = Hll::Dense(DenseRegisters::with_settings(
        &Settings::new(4, 5, 0, false).unwrap(),
    ));

    for i in 0..100 {
        hll.add_raw(i as u64);
    }

    assert!(hll.cardinality() > 0);
    hll.clear();
    assert_eq!(0, hll.cardinality());
}

#[test]
fn test_to_from_bytes_dense() {
    let settings = dense_test_settings();
    let expected_byte_count = 3 + divide_by_8_round_up(settings.reg_width * (1 << settings.log_2m));

    // Test empty element
    {
        let hll = Hll::Dense(DenseRegisters::with_settings(&settings));
        let bytes = hll.to_bytes();

        assert_eq!(expected_byte_count, bytes.len() as u32);

        let in_hll = Hll::from_bytes(&bytes).unwrap();
        assert_elements_equal_dense(&hll, &in_hll);
    }

    // Test partially filled element
    {
        let mut hll = Hll::new(settings);
        for i in 0..3 {
            hll.add_raw(construct_hll_value(settings.log_2m, i, i + 9));
        }
        let bytes = hll.to_bytes();

        assert_eq!(expected_byte_count, bytes.len() as u32);

        let in_hll = Hll::from_bytes(&bytes).unwrap();
        assert_elements_equal_dense(&hll, &in_hll);
    }

    // Test full set
    {
        let mut hll = Hll::new(settings);
        for i in 0..(1 << settings.log_2m) {
            hll.add_raw(construct_hll_value(settings.log_2m, i, (i % 9) + 1));
        }
        let bytes = hll.to_bytes();

        assert_eq!(expected_byte_count, bytes.len() as u32);

        let in_hll = Hll::from_bytes(&bytes).unwrap();
        assert_elements_equal_dense(&hll, &in_hll);
    }
}

#[test]
fn test_to_from_bytes_dense_trailing() {
    // Ensure coverage on code where number of bits is not evenly divisible by 64
    let settings = dense_test_settings();
    let mut hll = Hll::new(settings);

    for i in 0..16 {
        hll.add_raw(construct_hll_value(settings.log_2m, i, i + 1));
    }

    let bytes = hll.to_bytes();
    assert!(bytes.len() % 8 != 0);

    let hll2 = Hll::from_bytes(&bytes).unwrap();
    assert_eq!(hll, hll2);
}

#[test]
fn test_dense_registers() {
    let tests = vec![
        // Register width 4 (minimum size)
        (
            4,
            vec![
                0x000000000000001,  /*'j'=1*/
                0x0000000000000012, /*'j'=2*/
                0x0000000000000023, /*'j'=3*/
                0x0000000000000044, /*'j'=4*/
                0x0000000000000085, /*'j'=5*/
                0x0000000000010006, /*'j'=6*/
                0x0000000000020007, /*'j'=7*/
                0x0000000000040008, /*'j'=8*/
                0x0000000000080009, /*'j'=9*/
                0x000000000003000A, /*'j'=10*/
                0x000000000011000B, /*'j'=11*/
            ],
            vec![
                (1, 0),
                (2, 1),
                (3, 2),
                (4, 3),
                (5, 4),
                (6, 13),
                (7, 14),
                (8, 15),
                (9, 15), /*overflow*/
                (10, 13),
                (11, 13),
            ],
        ),
        // Register width 5
        (
            5,
            vec![
                0x000000000000001,  /*'j'=1*/
                0x0000000000000012, /*'j'=2*/
                0x0000000000000023, /*'j'=3*/
                0x0000000000000044, /*'j'=4*/
                0x0000000000000085, /*'j'=5*/
                0x0000000100000006, /*'j'=6*/
                0x0000000200000007, /*'j'=7*/
                0x0000000400000008, /*'j'=8*/
                0x0000000800000009, /*'j'=9*/
            ],
            vec![
                (1, 0),
                (2, 1),
                (3, 2),
                (4, 3),
                (5, 4),
                (6, 29),
                (7, 30),
                (8, 31),
                (9, 31), /*overflow*/
            ],
        ),
    ];

    let log_2m = 4;

    for (reg_width, values, registers) in tests {
        let mut hll = Hll::Dense(DenseRegisters::with_settings(
            &Settings::new(log_2m, reg_width, 0, false).unwrap(),
        ));

        for value in values {
            hll.add_raw(value);
        }

        for (regnum, value) in registers {
            if let Hll::Dense(dense) = &hll {
                assert_eq!(value as u8, dense.get(regnum));
            }
        }
    }
}

#[test]
fn test_dense_get() {
    let settings = Settings::new(11, 7, 0, false).unwrap();

    let mut hll = DenseRegisters::with_settings(&settings);
    for i in 0..(1 << settings.log_2m) {
        hll.set_if_greater(i, (i & (0x80 - 1)) as u8);
    }

    // let t = -4611686018427387904_i64;
    // let t2 = t as u64;

    for i in 0..(1 << settings.log_2m) {
        let expected = i & (0x80 - 1);
        assert_eq!(expected as u8, hll.get(i), "loop: {}", i);
    }
}

fn assert_elements_equal_dense(hll1: &Hll, hll2: &Hll) {
    assert!(assert_dense(hll1) && assert_dense(hll2));
    assert_eq!(hll1, hll2);
}

fn assert_dense(hll: &Hll) -> bool {
    matches!(hll, Hll::Dense(_))
}
