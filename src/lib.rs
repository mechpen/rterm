#[derive(Debug)]
pub struct Error {
    pub msg: String,
}

impl<T: std::fmt::Display> std::convert::From<T> for Error {
    fn from(e: T) -> Self {
        Error {
            msg: format!("{}", e),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

mod charset;
mod color;
mod cursor;
mod font;
mod glyph;
mod keymap;
mod point;
mod pty;
mod shell;
mod shortcut;
mod snap;
mod term;
mod utils;
mod vte;
mod win;
mod x11_wrapper;

pub mod app;
