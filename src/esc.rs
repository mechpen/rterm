// escape sequence parsing
// FIXME: support other esc sequences

use std::fmt;

use crate::utils::{
    is_between,
    atoi,
};

const ESC_BUF_SIZE: usize = 512;

// CSI Escape sequence structs
// ESC '[' [[ [<priv>] <arg> [;]] <mode> [<mode>]]
pub struct Csi {
    pub private: bool,
    pub args: Vec<usize>,
    pub mode: u8,
}

impl Csi {
    pub fn new(private: bool, args: Vec<usize>, mode: u8) -> Self {
        assert!(args.len() >= 1);
        Csi { private, args, mode }
    }
}

impl fmt::Display for Csi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f, "csi ({}, {:?}, {})", self.private, self.args, self.mode as char
        )
    }
}

fn parse_csi(buf: &[u8], mode: u8) -> Option<Csi> {
    let mut private = false;
    let mut args = Vec::new();
    let mut s = 0;

    if buf.len() > 0 && buf[s] == b'?' {
        s = 1;
        private = true;
    }

    for arg in buf[s..].split(|&x| x == b';') {
        if arg.len() == 0 {
            args.push(0);
            continue;
        }
        match atoi(arg.to_vec()) {
            Ok(x) => args.push(x),
            Err(_) => return None,
        }
    }

    Some(Csi::new(private, args, mode))
}

pub enum Esc {
    Eaten,
    Esc(u8),
    Csi(Csi),
}

enum EscState {
    None,
    Esc,
    Csi,
}

pub struct EscBuf {
    state: EscState,
    buf: Vec<u8>,
}

impl EscBuf {
    pub fn new() -> Self {
        EscBuf {
            state: EscState::None,
            buf: Vec::with_capacity(ESC_BUF_SIZE),
        }
    }

    pub fn start(&mut self) {
        self.state = EscState::Esc;
    }

    pub fn end(&mut self) {
        self.buf.truncate(0);
        self.state = EscState::None;
    }

    pub fn input(&mut self, c: u8) -> Option<Esc> {
        match (&self.state, c) {
            (EscState::None, _) => None,
            (EscState::Esc, b'[') => {
                self.state = EscState::Csi;
                Some(Esc::Eaten)
            },
            (EscState::Esc, c) => {
                self.end();
                Some(Esc::Esc(c))
            },
            (EscState::Csi, c) if is_between(c, 0x40, 0x7E) => {
                let csi = parse_csi(&self.buf, c);
                self.end();
                Some(csi.map_or(Esc::Eaten, |csi| { Esc::Csi(csi) }))
            },
            (_, c) if self.buf.len() < ESC_BUF_SIZE => {
                self.buf.push(c);
                Some(Esc::Eaten)
            },
            _ => {
                self.end();
                None
            },
        }
    }
}
