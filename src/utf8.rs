use crate::utils::is_between;

pub const UTF_SIZE:    usize = 4;

const UTF_INVALID: u32 = 0xFFFD;

const UTF_RESERVED_MIN: u32 = 0xD800;
const UTF_RESERVED_MAX: u32 = 0xDFFF;

const UTF_FIRST_MASK: [u8; UTF_SIZE] = [0x80, 0xE0, 0xF0, 0xF8];
const UTF_FIRST_CODE: [u8; UTF_SIZE] = [   0, 0xC0, 0xE0, 0xF0];
const UTF_CONT_MASK:  u8 = 0xC0;
const UTF_CONT_CODE:  u8 = 0x80;

const UTF_MIN: [u32; UTF_SIZE] = [   0,  0x80,  0x800,  0x10000];
const UTF_MAX: [u32; UTF_SIZE] = [0x7F, 0x7FF, 0xFFFF, 0x10FFFF];

#[inline]
fn is_reserved(u: u32) -> bool {
    is_between(u, UTF_RESERVED_MIN, UTF_RESERVED_MAX)
}

fn encode_len(u: u32) -> usize {
    for i in 0..UTF_SIZE {
        if u <= UTF_MAX[i] {
            return i + 1
        }
    }
    unreachable!();
}

pub fn encode(u: u32, buf: &mut [u8]) -> usize {
    let mut u = u;

    if u > UTF_MAX[UTF_SIZE-1] || is_reserved(u) {
        u = UTF_INVALID;
    }

    let len = encode_len(u);

    for i in (1..len).rev() {
        buf[i] = UTF_CONT_CODE | (u as u8 & !UTF_CONT_MASK);
        u >>= 6;
    }
    buf[0] = UTF_FIRST_CODE[len-1] | u as u8;

    return len
}

fn decode_first(c: u8, u: &mut u32) -> usize {
    for i in 0..UTF_SIZE {
        if c & UTF_FIRST_MASK[i] == UTF_FIRST_CODE[i] {
            *u = (c & !UTF_FIRST_MASK[i]) as u32;
            return i + 1;
        }
    }

    return 0;
}

pub fn decode(buf: &[u8], u: &mut u32) -> usize {
    *u = UTF_INVALID;
    let mut ud = 0;

    let len = decode_first(buf[0], &mut ud);
    if len == 0 {
        return 1;
    }
    if len > buf.len() {
        return 0;
    }

    for i in 1..len {
        let c = buf[i];
        if c & UTF_CONT_MASK != UTF_CONT_CODE {
            return i + 1;
        }
        ud = ud << 6 | (c & !UTF_CONT_MASK) as u32;
    }

    if is_between(ud, UTF_MIN[len-1], UTF_MAX[len-1]) && !is_reserved(ud) {
        *u = ud;
    }

    return len;
}

#[cfg(test)]
mod tests {
    const ENC: u32 = 1;
    const DEC: u32 = 2;

    #[test]
    fn encode_decode() {
        let samples: &[(u32, u32, usize, &[u8])] = &[
            // valid
            (ENC|DEC, 0x0,     1, &[0,    0,    0,    0]),
            (ENC|DEC, 0x80,    2, &[0xC2, 0x80, 0,    0]),
            (ENC|DEC, 0x800,   3, &[0xE0, 0xA0, 0x80, 0]),
            (ENC|DEC, 0x10000, 4, &[0xF0, 0x90, 0x80, 0x80]),

            // invalid range
            (ENC,     0xD800,   3, &[0xEF, 0xBF, 0xBD, 0]),
            (ENC,     0x110000, 3, &[0xEF, 0xBF, 0xBD, 0]),

            // invalid code
            (DEC,     0xFFFD,   1, &[0xF8, 0,    0,    0]),
            (DEC,     0xFFFD,   2, &[0xC2, 0xF0, 0,    0]),
            (DEC,     0xFFFD,   3, &[0xE0, 0x80, 0xF0, 0]),
            (DEC,     0xFFFD,   4, &[0xF0, 0x80, 0x80, 0xF0]),

            // invalid range
            (DEC,     0xFFFD,   2, &[0xC0, 0x80, 0,    0]),
            (DEC,     0xFFFD,   3, &[0xE0, 0x80, 0x80, 0]),
            (DEC,     0xFFFD,   4, &[0xF0, 0x80, 0x80, 0x80]),

            // 0 len
            (DEC,     0xFFFD,   0, &[0xF0, 0x90, 0x80]),
        ];

        samples.iter().for_each(|&(op, u, len, buf)| {
            if op & ENC != 0 {
                let mut buf_r = [0; 4];
                let len_r = super::encode(u, &mut buf_r);
                assert_eq!(len, len_r, "0x{:X}", u);
                for i in 0..len_r {
                    assert_eq!(buf[i], buf_r[i], "0x{:X} {}", u, i);
                }
            }

            if op & DEC != 0 {
                let mut u_r = 0;
                let len_r = super::decode(buf, &mut u_r);
                assert_eq!(len, len_r, "0x{:X}", u);
                assert_eq!(u, u_r, "0x{:X}", u);
            }
        })
    }
}
