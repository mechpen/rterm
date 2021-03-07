use std::cmp;

#[inline]
pub fn is_between<T: PartialOrd>(x: T, a: T, b: T) -> bool {
    a <= x && x <= b
}

#[inline]
pub fn limit<T: Ord>(x: T, min: T, max: T) -> T {
    cmp::min(cmp::max(x, min), max)
}

#[inline]
pub fn sort_pair<T: Ord>(a: T, b: T) -> (T, T) {
    if a > b { (b, a) } else { (a, b) }
}

#[inline]
pub fn is_control_c0(b: u8) -> bool {
    is_between(b, 0, 0x1F) || b == 0x7F
}

#[inline]
pub fn is_control_c1(b: u8) -> bool {
    is_between(b, 0x80, 0x9F)
}

#[inline]
pub fn is_control(b: u8) -> bool {
    is_control_c0(b) || is_control_c1(b)
}

pub fn term_decode(buf: &[u8]) -> String {
    let mut string = String::new();

    for &b in buf {
        let mut b = b;
        if is_control(b) {
            if b & 0x80 != 0 {
                b &= 0x7F;
                string.push('^');
                string.push('[');
            } else if !b"\n\r\t".contains(&b) {
                b ^= 0x40;
                string.push('^');
            }
        }
        string.push(b as char);
    }

    string
}
