use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

pub const COLOR_NAMES: &[&str] = &[
    /* 8 normal colors */
    "black",
    "red3",
    "green3",
    "yellow3",
    "blue2",
    "magenta3",
    "cyan3",
    "gray90",
    /* 8 bright colors */
    "gray50",
    "red",
    "green",
    "yellow",
    "#5c5cff",
    "magenta",
    "cyan",
    "white",
];

static FG_COLOR: AtomicUsize = AtomicUsize::new(7);
static BG_COLOR: AtomicUsize = AtomicUsize::new(0);

pub fn fg_color() -> usize {
    FG_COLOR.load(Ordering::Relaxed)
}

pub fn bg_color() -> usize {
    BG_COLOR.load(Ordering::Relaxed)
}
