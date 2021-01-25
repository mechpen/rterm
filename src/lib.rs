#![allow(unused)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

// FIXME: error handling use error code

#[derive(Debug)]
pub struct Error {
    pub msg: String,
}

impl<T: std::fmt::Display> std::convert::From<T> for Error {
    fn from(e: T) -> Self {
        Error{ msg: format!("{}", e) }
    }
}

macro_rules! error {
    ( $($args:tt)* ) => {
        {
            use $crate::Error;
            Error{ msg: format!($($args)*) }
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

mod sys;
mod esc;
mod utf8;
mod utils;
mod keymap;
mod snap;

pub mod win;
pub mod term;
