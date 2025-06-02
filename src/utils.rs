pub fn divide_by_8_round_up(i: u32) -> u32 {
    let mut result = i >> 3;
    if (i & 0x07) > 0 {
        result += 1;
    }
    result
}

/// (idx, pos)
#[inline(always)]
pub(crate) fn calc_position(reg_num: u32, reg_width: u8) -> (usize, u8) {
    let addr = reg_num * (reg_width as u32);
    let idx = addr >> 3; /*divide by 8*/
    let pos = addr & 0x07; /*remainder 8*/
    (idx.try_into().unwrap(), pos.try_into().unwrap())
}

#[inline(always)]
pub(crate) fn write_u8_bits(buf: &mut [u8], idx: usize, pos: u8, value: u8, n_bits: u8) {
    assert!(pos < 8);

    if pos + n_bits <= 8 {
        let mut mask: u32 = (1 << n_bits) - 1;
        let mask_offset = 8 - (pos + n_bits);
        mask <<= mask_offset;

        buf[idx] &= !(mask as u8);
        buf[idx] |= value << mask_offset;

        return;
    }

    // boundary write
    let n_bits_upper = 8 - pos;
    let n_bits_lower = n_bits - n_bits_upper;

    let mask_upper = (1 << n_bits_upper) - 1;
    let mask_lower = 0xFF >> n_bits_lower;

    // keep the bits still set at the beginning of idx and at the end of idx + 1. to do so we
    // clear the bits at the end of idx and beginning of idx +1 so that way they can be set by
    // the value.
    let upper_value = (value >> n_bits_lower) & mask_upper;
    let lower_value = value << (8 - n_bits_lower);

    buf[idx] = (buf[idx] & !mask_upper) | upper_value;
    buf[idx + 1] = (buf[idx + 1] & mask_lower) | lower_value;
}

#[inline(always)]
pub(crate) fn read_u8_bits(buf: &[u8], idx: usize, pos: u8, n_bits: u8) -> u8 {
    assert!(pos < 8);

    if pos + n_bits <= 8 {
        let mask: u32 = (1 << n_bits) - 1;
        let mask_offset = 8 - (pos + n_bits);

        return (buf[idx] >> mask_offset) & (mask as u8);
    }

    // boundary read
    let n_bits_upper = 8 - pos;
    let n_bits_lower = n_bits - n_bits_upper;

    let mask_upper = (1 << n_bits_upper) - 1;

    let upper = (buf[idx] & mask_upper) << n_bits_lower;
    let lower = buf[idx + 1] >> (8 - n_bits_lower);

    upper | lower
}

#[inline(always)]
pub(crate) fn write_bits(buf: &mut [u8], idx: usize, pos: u8, value: u32, n_bits: u8) {
    if n_bits == 0 {
        return;
    }

    let value_bytes = value.to_be_bytes();
    let vbyte_cnt = divide_by_8_round_up(n_bits as u32);
    let mut vidx = (4 - vbyte_cnt) as usize;

    let mut pos = pos;
    let mut idx = idx;
    let mut w_bits = n_bits & 0x07;
    if w_bits > 0 {
        write_u8_bits(buf, idx, pos, value_bytes[vidx], w_bits);
        if w_bits + pos > 0x07 {
            idx += 1;
        }
        pos = (w_bits + pos) & 0x07;
        vidx += 1;
    }

    w_bits = 0x08;
    while vidx < 4 {
        write_u8_bits(buf, idx, pos, value_bytes[vidx], w_bits);
        idx += 1;
        vidx += 1;
    }
}

#[inline(always)]
pub(crate) fn read_bits(buf: &[u8], idx: usize, pos: u8, n_bits: u8) -> u32 {
    let mut value = 0_u32.to_be_bytes();
    let vbyte_cnt = divide_by_8_round_up(n_bits as u32);
    let mut vidx = (4 - vbyte_cnt) as usize;

    let mut pos = pos;
    let mut idx = idx;
    let r_bits = n_bits & 0x07;
    if r_bits > 0 {
        value[vidx] = read_u8_bits(buf, idx, pos, r_bits);
        if r_bits + pos > 0x07 {
            idx += 1;
        }
        pos = (r_bits + pos) & 0x07;
        vidx += 1;
    }

    let r_bits = 0x08;
    while vidx < 4 {
        value[vidx] = read_u8_bits(buf, idx, pos, r_bits);
        idx += 1;
        vidx += 1;
    }

    u32::from_be_bytes(value)
}

#[cfg(test)]
mod test {
    use super::{read_bits, write_bits};

    #[test]
    fn rw_bits() {
        let mut buf = vec![0u8; 8];

        write_bits(&mut buf, 1, 2, 3, 4);
        assert_eq!([0, 12, 0, 0, 0, 0, 0, 0], buf.as_slice());

        let res = read_bits(&buf, 1, 2, 4);
        assert_eq!(3, res);

        write_bits(&mut buf, 1, 0, 0x0a, 8);
        assert_eq!([0, 0x0a, 0, 0, 0, 0, 0, 0], buf.as_slice());

        let res = read_bits(&buf, 1, 0, 8);
        assert_eq!(0x0a, res);

        write_bits(&mut buf, 0, 7, 0xFF, 8);
        assert_eq!([0x01, 0xFE, 0, 0, 0, 0, 0, 0], buf.as_slice());

        let res = read_bits(&buf, 0, 7, 8);
        assert_eq!(0xFF, res);

        write_bits(&mut buf, 2, 3, 0xFFFF, 15);
        assert_eq!([0x01, 0xFE, 31, 0xFF, 192, 0, 0, 0], buf.as_slice());

        let res = read_bits(&buf, 2, 3, 15);
        assert_eq!(0x7FFF, res);
    }

    #[test]
    #[should_panic]
    fn rw_bits_panic() {
        let mut buf = vec![0u8; 2];
        write_bits(&mut buf, 1, 7, 0xFFFF, 15)
    }
}
