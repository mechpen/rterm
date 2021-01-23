use std::cmp;
use std::str::FromStr;
use std::ops::{
    Bound,
    Range,
    RangeBounds,
};

use crate::Result;

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
pub fn is_set(a: u32, b: u32) -> bool {
    a & b != 0
}

#[inline]
pub fn mod_flag(mode: &mut u32, set: bool, flag: u32) {
    *mode = if set { *mode | flag } else { *mode & !flag }
}

#[inline]
pub fn assert_range<R: RangeBounds<usize>>(rb: &R) -> Range<usize>
{
    let start = match rb.start_bound() {
        Bound::Included(&x) => x,
        Bound::Excluded(&x) => x+1,
        Bound::Unbounded => panic!("unbound range"),
    };
    let end = match rb.end_bound() {
        Bound::Included(&x) => x+1,
        Bound::Excluded(&x) => x,
        Bound::Unbounded => panic!("unbound range"),
    };
    start..end
}

#[inline]
pub fn is_control_c0(u: u8) -> bool {
    is_between(u, 0, 0x1F) || u == 0x7F
}

#[inline]
pub fn is_control_c1(u: u8) -> bool {
    is_between(u, 0x80, 0x9F)
}

#[inline]
pub fn is_control(u: u8) -> bool {
    is_control_c0(u) || is_control_c1(u)
}

#[inline]
pub fn atoi<T: FromStr>(buf: Vec<u8>) -> Result<T> {
    let s = String::from_utf8(buf)?;
    match s.parse::<T>() {
        Ok(x) => Ok(x),
        Err(_) => Err("".into()),
    }
}
