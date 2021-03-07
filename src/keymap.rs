use x11::xlib::*;
use x11::keysym::*;
use std::os::raw::*;
use crate::win::Mode;

const XK_ANY_MOD:    u32 = u32::MAX;
const XK_NO_MOD:     u32 = 0;
const XK_SWITCH_MOD: u32 = 1<<13;
const IGNORE_MOD:    u32 = Mod2Mask | XK_SWITCH_MOD;

struct Key {
    k: c_uint,
    mask: c_uint,
    s: &'static [u8],
    /* three-valued logic variables: 0 indifferent, 1 on, -1 off */
    appkeypad: c_char, /* application keypad */
    appcursor: c_char, /* application cursor */
}

macro_rules! make_keys {
    {
        $({ $k:expr, $mask:expr, $s:expr, $appkeypad:expr, $appcursor:expr },)*
    } => {
        &[
            $(Key {
                k: $k,
                mask: $mask,
                s: $s,
                appkeypad: $appkeypad,
                appcursor: $appcursor,
            },)*
        ]
    }
}

const KEYS: &[Key] = make_keys!{
    /* keysym           mask            string      appkeypad appcursor */
    { XK_KP_Home,       ShiftMask,      b"\x1B[2J",       0,   -1},
    { XK_KP_Home,       ShiftMask,      b"\x1B[1;2H",     0,    1},
    { XK_KP_Home,       XK_ANY_MOD,     b"\x1B[H",        0,   -1},
    { XK_KP_Home,       XK_ANY_MOD,     b"\x1B[1~",       0,    1},
    { XK_KP_Up,         XK_ANY_MOD,     b"\x1BOx",        1,    0},
    { XK_KP_Up,         XK_ANY_MOD,     b"\x1B[A",        0,   -1},
    { XK_KP_Up,         XK_ANY_MOD,     b"\x1BOA",        0,    1},
    { XK_KP_Down,       XK_ANY_MOD,     b"\x1BOr",        1,    0},
    { XK_KP_Down,       XK_ANY_MOD,     b"\x1B[B",        0,   -1},
    { XK_KP_Down,       XK_ANY_MOD,     b"\x1BOB",        0,    1},
    { XK_KP_Left,       XK_ANY_MOD,     b"\x1BOt",        1,    0},
    { XK_KP_Left,       XK_ANY_MOD,     b"\x1B[D",        0,   -1},
    { XK_KP_Left,       XK_ANY_MOD,     b"\x1BOD",        0,    1},
    { XK_KP_Right,      XK_ANY_MOD,     b"\x1BOv",        1,    0},
    { XK_KP_Right,      XK_ANY_MOD,     b"\x1B[C",        0,   -1},
    { XK_KP_Right,      XK_ANY_MOD,     b"\x1BOC",        0,    1},
    { XK_KP_Prior,      ShiftMask,      b"\x1B[5;2~",     0,    0},
    { XK_KP_Prior,      XK_ANY_MOD,     b"\x1B[5~",       0,    0},
    { XK_KP_Begin,      XK_ANY_MOD,     b"\x1B[E",        0,    0},
    { XK_KP_End,        ControlMask,    b"\x1B[J",       -1,    0},
    { XK_KP_End,        ControlMask,    b"\x1B[1;5F",     1,    0},
    { XK_KP_End,        ShiftMask,      b"\x1B[K",       -1,    0},
    { XK_KP_End,        ShiftMask,      b"\x1B[1;2F",     1,    0},
    { XK_KP_End,        XK_ANY_MOD,     b"\x1B[4~",       0,    0},
    { XK_KP_Next,       ShiftMask,      b"\x1B[6;2~",     0,    0},
    { XK_KP_Next,       XK_ANY_MOD,     b"\x1B[6~",       0,    0},
    { XK_KP_Insert,     ShiftMask,      b"\x1B[2;2~",     1,    0},
    { XK_KP_Insert,     ShiftMask,      b"\x1B[4l",      -1,    0},
    { XK_KP_Insert,     ControlMask,    b"\x1B[L",       -1,    0},
    { XK_KP_Insert,     ControlMask,    b"\x1B[2;5~",     1,    0},
    { XK_KP_Insert,     XK_ANY_MOD,     b"\x1B[4h",      -1,    0},
    { XK_KP_Insert,     XK_ANY_MOD,     b"\x1B[2~",       1,    0},
    { XK_KP_Delete,     ControlMask,    b"\x1B[M",       -1,    0},
    { XK_KP_Delete,     ControlMask,    b"\x1B[3;5~",     1,    0},
    { XK_KP_Delete,     ShiftMask,      b"\x1B[2K",      -1,    0},
    { XK_KP_Delete,     ShiftMask,      b"\x1B[3;2~",     1,    0},
    { XK_KP_Delete,     XK_ANY_MOD,     b"\x1B[P",       -1,    0},
    { XK_KP_Delete,     XK_ANY_MOD,     b"\x1B[3~",       1,    0},
    { XK_KP_Multiply,   XK_ANY_MOD,     b"\x1BOj",        2,    0},
    { XK_KP_Add,        XK_ANY_MOD,     b"\x1BOk",        2,    0},
    { XK_KP_Enter,      XK_ANY_MOD,     b"\x1BOM",        2,    0},
    { XK_KP_Enter,      XK_ANY_MOD,     b"\r",           -1,    0},
    { XK_KP_Subtract,   XK_ANY_MOD,     b"\x1BOm",        2,    0},
    { XK_KP_Decimal,    XK_ANY_MOD,     b"\x1BOn",        2,    0},
    { XK_KP_Divide,     XK_ANY_MOD,     b"\x1BOo",        2,    0},
    { XK_KP_0,          XK_ANY_MOD,     b"\x1BOp",        2,    0},
    { XK_KP_1,          XK_ANY_MOD,     b"\x1BOq",        2,    0},
    { XK_KP_2,          XK_ANY_MOD,     b"\x1BOr",        2,    0},
    { XK_KP_3,          XK_ANY_MOD,     b"\x1BOs",        2,    0},
    { XK_KP_4,          XK_ANY_MOD,     b"\x1BOt",        2,    0},
    { XK_KP_5,          XK_ANY_MOD,     b"\x1BOu",        2,    0},
    { XK_KP_6,          XK_ANY_MOD,     b"\x1BOv",        2,    0},
    { XK_KP_7,          XK_ANY_MOD,     b"\x1BOw",        2,    0},
    { XK_KP_8,          XK_ANY_MOD,     b"\x1BOx",        2,    0},
    { XK_KP_9,          XK_ANY_MOD,     b"\x1BOy",        2,    0},
    { XK_Up,            ShiftMask,      b"\x1B[1;2A",     0,    0},
    { XK_Up,            Mod1Mask,       b"\x1B[1;3A",     0,    0},
    { XK_Up,         ShiftMask|Mod1Mask,b"\x1B[1;4A",     0,    0},
    { XK_Up,            ControlMask,    b"\x1B[1;5A",     0,    0},
    { XK_Up,      ShiftMask|ControlMask,b"\x1B[1;6A",     0,    0},
    { XK_Up,       ControlMask|Mod1Mask,b"\x1B[1;7A",     0,    0},
    { XK_Up,ShiftMask|ControlMask|Mod1Mask,b"\x1B[1;8A",  0,    0},
    { XK_Up,            XK_ANY_MOD,     b"\x1B[A",        0,   -1},
    { XK_Up,            XK_ANY_MOD,     b"\x1BOA",        0,    1},
    { XK_Down,          ShiftMask,      b"\x1B[1;2B",     0,    0},
    { XK_Down,          Mod1Mask,       b"\x1B[1;3B",     0,    0},
    { XK_Down,       ShiftMask|Mod1Mask,b"\x1B[1;4B",     0,    0},
    { XK_Down,          ControlMask,    b"\x1B[1;5B",     0,    0},
    { XK_Down,    ShiftMask|ControlMask,b"\x1B[1;6B",     0,    0},
    { XK_Down,     ControlMask|Mod1Mask,b"\x1B[1;7B",     0,    0},
    { XK_Down,ShiftMask|ControlMask|Mod1Mask,b"\x1B[1;8B",0,    0},
    { XK_Down,          XK_ANY_MOD,     b"\x1B[B",        0,   -1},
    { XK_Down,          XK_ANY_MOD,     b"\x1BOB",        0,    1},
    { XK_Left,          ShiftMask,      b"\x1B[1;2D",     0,    0},
    { XK_Left,          Mod1Mask,       b"\x1B[1;3D",     0,    0},
    { XK_Left,       ShiftMask|Mod1Mask,b"\x1B[1;4D",     0,    0},
    { XK_Left,          ControlMask,    b"\x1B[1;5D",     0,    0},
    { XK_Left,    ShiftMask|ControlMask,b"\x1B[1;6D",     0,    0},
    { XK_Left,     ControlMask|Mod1Mask,b"\x1B[1;7D",     0,    0},
    { XK_Left,ShiftMask|ControlMask|Mod1Mask,b"\x1B[1;8D",0,    0},
    { XK_Left,          XK_ANY_MOD,     b"\x1B[D",        0,   -1},
    { XK_Left,          XK_ANY_MOD,     b"\x1BOD",        0,    1},
    { XK_Right,         ShiftMask,      b"\x1B[1;2C",     0,    0},
    { XK_Right,         Mod1Mask,       b"\x1B[1;3C",     0,    0},
    { XK_Right,      ShiftMask|Mod1Mask,b"\x1B[1;4C",     0,    0},
    { XK_Right,         ControlMask,    b"\x1B[1;5C",     0,    0},
    { XK_Right,   ShiftMask|ControlMask,b"\x1B[1;6C",     0,    0},
    { XK_Right,    ControlMask|Mod1Mask,b"\x1B[1;7C",     0,    0},
    { XK_Right,ShiftMask|ControlMask|Mod1Mask,b"\x1B[1;8C",0,   0},
    { XK_Right,         XK_ANY_MOD,     b"\x1B[C",        0,   -1},
    { XK_Right,         XK_ANY_MOD,     b"\x1BOC",        0,    1},
    { XK_ISO_Left_Tab,  ShiftMask,      b"\x1B[Z",        0,    0},
    { XK_Return,        Mod1Mask,       b"\x1B\r",        0,    0},
    { XK_Return,        XK_ANY_MOD,     b"\r",            0,    0},
    { XK_Insert,        ShiftMask,      b"\x1B[4l",      -1,    0},
    { XK_Insert,        ShiftMask,      b"\x1B[2;2~",     1,    0},
    { XK_Insert,        ControlMask,    b"\x1B[L",       -1,    0},
    { XK_Insert,        ControlMask,    b"\x1B[2;5~",     1,    0},
    { XK_Insert,        XK_ANY_MOD,     b"\x1B[4h",      -1,    0},
    { XK_Insert,        XK_ANY_MOD,     b"\x1B[2~",       1,    0},
    { XK_Delete,        ControlMask,    b"\x1B[M",       -1,    0},
    { XK_Delete,        ControlMask,    b"\x1B[3;5~",     1,    0},
    { XK_Delete,        ShiftMask,      b"\x1B[2K",      -1,    0},
    { XK_Delete,        ShiftMask,      b"\x1B[3;2~",     1,    0},
    { XK_Delete,        XK_ANY_MOD,     b"\x1B[P",       -1,    0},
    { XK_Delete,        XK_ANY_MOD,     b"\x1B[3~",       1,    0},
    { XK_BackSpace,     XK_NO_MOD,      b"\x7F",          0,    0},
    { XK_BackSpace,     Mod1Mask,       b"\x1B\x7F",      0,    0},
    { XK_Home,          ShiftMask,      b"\x1B[2J",       0,   -1},
    { XK_Home,          ShiftMask,      b"\x1B[1;2H",     0,    1},
    { XK_Home,          XK_ANY_MOD,     b"\x1B[H",        0,   -1},
    { XK_Home,          XK_ANY_MOD,     b"\x1B[1~",       0,    1},
    { XK_End,           ControlMask,    b"\x1B[J",       -1,    0},
    { XK_End,           ControlMask,    b"\x1B[1;5F",     1,    0},
    { XK_End,           ShiftMask,      b"\x1B[K",       -1,    0},
    { XK_End,           ShiftMask,      b"\x1B[1;2F",     1,    0},
    { XK_End,           XK_ANY_MOD,     b"\x1B[4~",       0,    0},
    { XK_Prior,         ControlMask,    b"\x1B[5;5~",     0,    0},
    { XK_Prior,         ShiftMask,      b"\x1B[5;2~",     0,    0},
    { XK_Prior,         XK_ANY_MOD,     b"\x1B[5~",       0,    0},
    { XK_Next,          ControlMask,    b"\x1B[6;5~",     0,    0},
    { XK_Next,          ShiftMask,      b"\x1B[6;2~",     0,    0},
    { XK_Next,          XK_ANY_MOD,     b"\x1B[6~",       0,    0},
    { XK_F1,            XK_NO_MOD,      b"\x1BOP" ,       0,    0},
    { XK_F1, /* F13 */  ShiftMask,      b"\x1B[1;2P",     0,    0},
    { XK_F1, /* F25 */  ControlMask,    b"\x1B[1;5P",     0,    0},
    { XK_F1, /* F37 */  Mod4Mask,       b"\x1B[1;6P",     0,    0},
    { XK_F1, /* F49 */  Mod1Mask,       b"\x1B[1;3P",     0,    0},
    { XK_F1, /* F61 */  Mod3Mask,       b"\x1B[1;4P",     0,    0},
    { XK_F2,            XK_NO_MOD,      b"\x1BOQ" ,       0,    0},
    { XK_F2, /* F14 */  ShiftMask,      b"\x1B[1;2Q",     0,    0},
    { XK_F2, /* F26 */  ControlMask,    b"\x1B[1;5Q",     0,    0},
    { XK_F2, /* F38 */  Mod4Mask,       b"\x1B[1;6Q",     0,    0},
    { XK_F2, /* F50 */  Mod1Mask,       b"\x1B[1;3Q",     0,    0},
    { XK_F2, /* F62 */  Mod3Mask,       b"\x1B[1;4Q",     0,    0},
    { XK_F3,            XK_NO_MOD,      b"\x1BOR" ,       0,    0},
    { XK_F3, /* F15 */  ShiftMask,      b"\x1B[1;2R",     0,    0},
    { XK_F3, /* F27 */  ControlMask,    b"\x1B[1;5R",     0,    0},
    { XK_F3, /* F39 */  Mod4Mask,       b"\x1B[1;6R",     0,    0},
    { XK_F3, /* F51 */  Mod1Mask,       b"\x1B[1;3R",     0,    0},
    { XK_F3, /* F63 */  Mod3Mask,       b"\x1B[1;4R",     0,    0},
    { XK_F4,            XK_NO_MOD,      b"\x1BOS" ,       0,    0},
    { XK_F4, /* F16 */  ShiftMask,      b"\x1B[1;2S",     0,    0},
    { XK_F4, /* F28 */  ControlMask,    b"\x1B[1;5S",     0,    0},
    { XK_F4, /* F40 */  Mod4Mask,       b"\x1B[1;6S",     0,    0},
    { XK_F4, /* F52 */  Mod1Mask,       b"\x1B[1;3S",     0,    0},
    { XK_F5,            XK_NO_MOD,      b"\x1B[15~",      0,    0},
    { XK_F5, /* F17 */  ShiftMask,      b"\x1B[15;2~",    0,    0},
    { XK_F5, /* F29 */  ControlMask,    b"\x1B[15;5~",    0,    0},
    { XK_F5, /* F41 */  Mod4Mask,       b"\x1B[15;6~",    0,    0},
    { XK_F5, /* F53 */  Mod1Mask,       b"\x1B[15;3~",    0,    0},
    { XK_F6,            XK_NO_MOD,      b"\x1B[17~",      0,    0},
    { XK_F6, /* F18 */  ShiftMask,      b"\x1B[17;2~",    0,    0},
    { XK_F6, /* F30 */  ControlMask,    b"\x1B[17;5~",    0,    0},
    { XK_F6, /* F42 */  Mod4Mask,       b"\x1B[17;6~",    0,    0},
    { XK_F6, /* F54 */  Mod1Mask,       b"\x1B[17;3~",    0,    0},
    { XK_F7,            XK_NO_MOD,      b"\x1B[18~",      0,    0},
    { XK_F7, /* F19 */  ShiftMask,      b"\x1B[18;2~",    0,    0},
    { XK_F7, /* F31 */  ControlMask,    b"\x1B[18;5~",    0,    0},
    { XK_F7, /* F43 */  Mod4Mask,       b"\x1B[18;6~",    0,    0},
    { XK_F7, /* F55 */  Mod1Mask,       b"\x1B[18;3~",    0,    0},
    { XK_F8,            XK_NO_MOD,      b"\x1B[19~",      0,    0},
    { XK_F8, /* F20 */  ShiftMask,      b"\x1B[19;2~",    0,    0},
    { XK_F8, /* F32 */  ControlMask,    b"\x1B[19;5~",    0,    0},
    { XK_F8, /* F44 */  Mod4Mask,       b"\x1B[19;6~",    0,    0},
    { XK_F8, /* F56 */  Mod1Mask,       b"\x1B[19;3~",    0,    0},
    { XK_F9,            XK_NO_MOD,      b"\x1B[20~",      0,    0},
    { XK_F9, /* F21 */  ShiftMask,      b"\x1B[20;2~",    0,    0},
    { XK_F9, /* F33 */  ControlMask,    b"\x1B[20;5~",    0,    0},
    { XK_F9, /* F45 */  Mod4Mask,       b"\x1B[20;6~",    0,    0},
    { XK_F9, /* F57 */  Mod1Mask,       b"\x1B[20;3~",    0,    0},
    { XK_F10,           XK_NO_MOD,      b"\x1B[21~",      0,    0},
    { XK_F10, /* F22 */ ShiftMask,      b"\x1B[21;2~",    0,    0},
    { XK_F10, /* F34 */ ControlMask,    b"\x1B[21;5~",    0,    0},
    { XK_F10, /* F46 */ Mod4Mask,       b"\x1B[21;6~",    0,    0},
    { XK_F10, /* F58 */ Mod1Mask,       b"\x1B[21;3~",    0,    0},
    { XK_F11,           XK_NO_MOD,      b"\x1B[23~",      0,    0},
    { XK_F11, /* F23 */ ShiftMask,      b"\x1B[23;2~",    0,    0},
    { XK_F11, /* F35 */ ControlMask,    b"\x1B[23;5~",    0,    0},
    { XK_F11, /* F47 */ Mod4Mask,       b"\x1B[23;6~",    0,    0},
    { XK_F11, /* F59 */ Mod1Mask,       b"\x1B[23;3~",    0,    0},
    { XK_F12,           XK_NO_MOD,      b"\x1B[24~",      0,    0},
    { XK_F12, /* F24 */ ShiftMask,      b"\x1B[24;2~",    0,    0},
    { XK_F12, /* F36 */ ControlMask,    b"\x1B[24;5~",    0,    0},
    { XK_F12, /* F48 */ Mod4Mask,       b"\x1B[24;6~",    0,    0},
    { XK_F12, /* F60 */ Mod1Mask,       b"\x1B[24;3~",    0,    0},
    { XK_F13,           XK_NO_MOD,      b"\x1B[1;2P",     0,    0},
    { XK_F14,           XK_NO_MOD,      b"\x1B[1;2Q",     0,    0},
    { XK_F15,           XK_NO_MOD,      b"\x1B[1;2R",     0,    0},
    { XK_F16,           XK_NO_MOD,      b"\x1B[1;2S",     0,    0},
    { XK_F17,           XK_NO_MOD,      b"\x1B[15;2~",    0,    0},
    { XK_F18,           XK_NO_MOD,      b"\x1B[17;2~",    0,    0},
    { XK_F19,           XK_NO_MOD,      b"\x1B[18;2~",    0,    0},
    { XK_F20,           XK_NO_MOD,      b"\x1B[19;2~",    0,    0},
    { XK_F21,           XK_NO_MOD,      b"\x1B[20;2~",    0,    0},
    { XK_F22,           XK_NO_MOD,      b"\x1B[21;2~",    0,    0},
    { XK_F23,           XK_NO_MOD,      b"\x1B[23;2~",    0,    0},
    { XK_F24,           XK_NO_MOD,      b"\x1B[24;2~",    0,    0},
    { XK_F25,           XK_NO_MOD,      b"\x1B[1;5P",     0,    0},
    { XK_F26,           XK_NO_MOD,      b"\x1B[1;5Q",     0,    0},
    { XK_F27,           XK_NO_MOD,      b"\x1B[1;5R",     0,    0},
    { XK_F28,           XK_NO_MOD,      b"\x1B[1;5S",     0,    0},
    { XK_F29,           XK_NO_MOD,      b"\x1B[15;5~",    0,    0},
    { XK_F30,           XK_NO_MOD,      b"\x1B[17;5~",    0,    0},
    { XK_F31,           XK_NO_MOD,      b"\x1B[18;5~",    0,    0},
    { XK_F32,           XK_NO_MOD,      b"\x1B[19;5~",    0,    0},
    { XK_F33,           XK_NO_MOD,      b"\x1B[20;5~",    0,    0},
    { XK_F34,           XK_NO_MOD,      b"\x1B[21;5~",    0,    0},
    { XK_F35,           XK_NO_MOD,      b"\x1B[23;5~",    0,    0},
};

pub fn map_key(k: KeySym, state: c_uint, mode: &Mode) -> Option<&'static [u8]> {
    let k = k as c_uint;
    if k & 0xFFFF < 0xFD00 {
        return None;
    }

    let state = state & !IGNORE_MOD;
    let numlock = mode.contains(Mode::NUMLOCK);
    let appkeypad = mode.contains(Mode::APPKEYPAD);
    let appcursor = mode.contains(Mode::APPCURSOR);

    for key in KEYS {
        if key.k != k {
            continue;
        }
        if key.mask != XK_ANY_MOD && key.mask != state {
            continue;
        }
        if numlock && key.appkeypad == 2 {
            continue;
        }

        if appkeypad {
            if key.appkeypad < 0 {
                continue;
            }
        } else {
            if key.appkeypad > 0 {
                continue;
            }
        }

        if appcursor {
            if key.appcursor < 0 {
                continue;
            }
        } else {
            if key.appcursor > 0 {
                continue;
            }
        }

        return Some(key.s);
    }

    None
}
