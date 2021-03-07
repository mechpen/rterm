#![allow(unused)]

#[derive(Debug)]
pub struct Error {
    pub msg: String,
}

impl<T: std::fmt::Display> std::convert::From<T> for Error {
    fn from(e: T) -> Self {
        Error{ msg: format!("{}", e) }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

mod x11_wrapper;
mod point;
mod glyph;
mod utils;
mod shortcut;
mod keymap;
mod snap;
mod shell;
mod cursor;
mod color;
mod term;
mod win;
mod vte;
mod pty;

pub mod app;
