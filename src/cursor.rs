use crate::glyph::{blank_glyph, Glyph};
use std::mem;

#[derive(Clone, Copy, Debug)]
pub struct Cursor {
    pub glyph: Glyph,
    pub x: usize,
    pub y: usize,
    // When cursor is at the right edge of the terminal, it does
    // not wrap to new line until one more character is input.
    pub wrap_next: bool,
}

impl Cursor {
    pub fn new() -> Self {
        Cursor {
            glyph: blank_glyph(),
            wrap_next: false,
            x: 0,
            y: 0,
        }
    }

    pub fn reset(&mut self) {
        let _ = mem::replace(self, Cursor::new());
    }
}
