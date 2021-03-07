use std::mem;
use crate::glyph::{
    Glyph,
    blank_glyph,
};

pub struct Cursor {
    pub glyph: Glyph,
    pub x: usize,
    pub y: usize,
    // When cursor is at the right edge of the terminal, it does
    // not wrap to new line until one more character is input.
    pub wrap_next: bool,

    saved_x: usize,
    saved_y: usize,
}

impl Cursor {
    pub fn new() -> Self {
        Cursor {
            glyph: blank_glyph(),
            wrap_next: false,
            x: 0, y: 0,
            saved_x: 0, saved_y: 0,
        }
    }

    pub fn reset(&mut self) {
        let _ = mem::replace(self, Cursor::new());
    }

    pub fn save_pos(&mut self) {
        self.saved_x = self.x;
        self.saved_y = self.y;
    }

    pub fn load_pos(&mut self) {
        self.x = self.saved_x;
        self.y = self.saved_y;
    }
}
