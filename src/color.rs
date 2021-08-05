// This covers the initial 16 colors, it also support 256 and true color modes
// and a palette of of the colors from 16-255 are generated (see Win::new() in
// win.rs).  The foreground/background and cursor get there own 'slots' after
// the 256 color palette.
pub const COLOR_NAMES: &[&str] = &[
    /* 8 normal colors */
    "black", "red3", "green3", "yellow3", "blue2", "magenta3", "cyan3", "gray90",
    /* 8 bright colors */
    "gray50", "red", "green", "yellow", "#5c5cff", "magenta", "cyan", "white",
];

pub const FG_COLOR: usize = 258;
pub const BG_COLOR: usize = 259;
pub const CURSOR_COLOR: usize = 256;
pub const CURSOR_REV_COLOR: usize = 257;
pub const FG_COLOR_NAME: &str = "grey90";
pub const BG_COLOR_NAME: &str = "black";
pub const CURSOR_COLOR_NAME: &str = "#cccccc";
pub const CURSOR_REV_COLOR_NAME: &str = "#555555";
