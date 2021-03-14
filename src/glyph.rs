use bitflags::bitflags;
use std::mem;
use crate::color::{
    fg_color,
    bg_color,
};

bitflags! {
    pub struct GlyphAttr: u16 {
        // font modifiers
        const BOLD       = 1 << 0;
        const FAINT      = 1 << 1;
        const ITALIC     = 1 << 2;
        const UNDERLINE  = 1 << 3;
        const BLINK      = 1 << 4;
        const REVERSE    = 1 << 5;
        const INVISIBLE  = 1 << 6;
        const STRUCK     = 1 << 7;

        const FONT_MASK  = (1 << 8) - 1;
        const BOLD_FAINT = Self::BOLD.bits | Self::FAINT.bits;

        // at line wrap
        const WRAP       = 1 << 8;

        // dummy for wide chars
        const DUMMY      = 1 << 9;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphProp {
    pub fg: usize,
    pub bg: usize,
    pub attr: GlyphAttr,
}

impl GlyphProp {
    pub fn new(fg: usize, bg: usize, attr: GlyphAttr) -> Self {
        Self { fg, bg, attr }
    }

    pub fn reset(&mut self) {
        self.fg = fg_color();
        self.bg = bg_color();
        self.attr -= GlyphAttr::FONT_MASK;
    }

    pub fn reset_fg(&mut self) {
        self.fg = fg_color();
    }

    pub fn reset_bg(&mut self) {
        self.bg = bg_color();
    }

    pub fn resolve(&self, reverse: bool) -> Self {
        let (mut fg, mut bg) = (self.fg, self.bg);
        let mut attr = self.attr & GlyphAttr::FONT_MASK;

        if reverse ^ attr.contains(GlyphAttr::REVERSE) {
            mem::swap(&mut fg, &mut bg);
        }
        attr.remove(GlyphAttr::REVERSE);

        if attr.contains(GlyphAttr::BOLD_FAINT) {
            attr.remove(GlyphAttr::BOLD_FAINT);
        }

        Self::new(fg, bg, attr)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub c: char,
    pub prop: GlyphProp,
}

impl Glyph {
    pub fn new(c: char, fg: usize, bg: usize, attr: GlyphAttr) -> Self {
        Self { c, prop: GlyphProp::new(fg, bg, attr) }
    }

    pub fn clear(&mut self, cursor: Glyph) {
        self.c = ' ';
        self.prop.fg = cursor.prop.fg;
        self.prop.bg = cursor.prop.bg;
        self.prop.attr = GlyphAttr::empty();
    }
}

pub fn blank_glyph() -> Glyph {
    Glyph::new(' ', fg_color(), bg_color(), GlyphAttr::empty())
}
