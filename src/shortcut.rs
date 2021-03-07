use x11::xlib::*;
use x11::keysym::*;
use std::os::raw::*;
use crate::win::Win;
use crate::term::Term;

#[derive(Clone, Copy)]
pub enum Function {
    Paste,
}

impl Function {
    pub fn execute(&self, win: &mut Win, term: &mut Term) {
        match self {
            Function::Paste => win.selection_paste(),
        }
    }
}

struct Shortcut {
    k: c_uint,
    mask: c_uint,
    function: Function,
}

macro_rules! make_shortcuts {
    {
        $({ $mask:expr, $k:expr, $function:expr },)*
    } => {
        &[
            $(Shortcut {
                k: $k,
                mask: $mask,
                function: $function,
            },)*
        ]
    }
}

const SHORTCUTS: &[Shortcut] = make_shortcuts!{
    /* mask                 keysym          function */
    { ShiftMask,            XK_Insert,      Function::Paste },
};

pub fn find_shortcut(k: KeySym, state: c_uint) -> Option<Function> {
    let k = k as c_uint;
    if k & 0xFFFF < 0xFD00 {
        return None;
    }

    for shortcut in SHORTCUTS {
        if k == shortcut.k && state & shortcut.mask != 0 {
            return Some(shortcut.function);
        }
    }
    None
}
