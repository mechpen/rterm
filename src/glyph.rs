use crate::color::{BG_COLOR, FG_COLOR};
use bitflags::bitflags;
use std::mem;

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

        // Indicates a wide glyph, an extra column with the DUMMY flag set will
        // be there for the colums past 1 (ie, [WIDE, DUMMY...]).
        const WIDE       = 1 << 9;
        // dummy for wide chars
        const DUMMY      = 1 << 10;
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
        self.fg = FG_COLOR;
        self.bg = BG_COLOR;
        self.attr -= GlyphAttr::FONT_MASK;
    }

    pub fn reset_fg(&mut self) {
        self.fg = FG_COLOR;
    }

    pub fn reset_bg(&mut self) {
        self.bg = BG_COLOR;
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
        Self {
            c,
            prop: GlyphProp::new(fg, bg, attr),
        }
    }

    pub fn clear(&mut self, cursor: Glyph) {
        self.c = ' ';
        self.prop.fg = cursor.prop.fg;
        self.prop.bg = cursor.prop.bg;
        self.prop.attr = GlyphAttr::empty();
    }
}

pub fn blank_glyph() -> Glyph {
    Glyph::new(' ', FG_COLOR, BG_COLOR, GlyphAttr::empty())
}
