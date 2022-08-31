use crate::Result;
use std::cmp;
use std::time::SystemTime;

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
    if a > b {
        (b, a)
    } else {
        (a, b)
    }
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

#[inline]
pub fn epoch_ms() -> i64 {
    SystemTime::now()
	.duration_since(SystemTime::UNIX_EPOCH)
	.unwrap()
	.as_millis() as i64
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

pub fn parse_geometry(s: &str) -> Result<(usize, usize, usize, usize)> {
    let mut xoff = 0;
    let mut yoff = 0;

    let fields = s.split('+').collect::<Vec<&str>>();
    if fields.len() == 3 {
        xoff = fields[1].parse::<usize>()?;
        yoff = fields[2].parse::<usize>()?;
    }

    let s = fields[0];
    let fields = s.split('x').collect::<Vec<&str>>();
    if fields.len() != 2 {
        return Err("invalid geometry".into());
    }

    let cols = fields[0].parse::<usize>()?;
    let rows = fields[1].parse::<usize>()?;
    Ok((cols, rows, xoff, yoff))
}
