use std::mem;

#[derive(Clone, Copy, Debug)]
pub enum CursorMode {
    Block,
    Underline,
    Bar,
}

#[derive(Clone, Copy, Debug)]
pub struct Cursor {
    pub mode: CursorMode,
    pub blink: bool,
    pub x: usize,
    pub y: usize,
    // When cursor is at the right edge of the terminal, it does
    // not wrap to new line until one more character is input.
    pub wrap_next: bool,
}

impl Cursor {
    pub fn new() -> Self {
        Cursor {
            mode: CursorMode::Block,
            blink: false,
            wrap_next: false,
            x: 0,
            y: 0,
        }
    }

    pub fn reset(&mut self) {
        let _ = mem::replace(self, Cursor::new());
    }
}
