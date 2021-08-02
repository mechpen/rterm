use crate::glyph::GlyphAttr;
use crate::x11_wrapper as x11;
use crate::Result;
use std::ffi::CString;
use std::os::raw::c_int;

/*
 * Printable characters in ASCII, used to estimate the advance width
 * of single wide characters.
 */
static ASCII_PRINTABLE: &[u8; 95] = b" !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";

pub struct Font {
    height: usize,
    width: usize,
    font: x11::XftFont,
    bfont: x11::XftFont,
    ifont: x11::XftFont,
    ibfont: x11::XftFont,
}

impl Font {
    pub fn new(dpy: x11::Display, scr: c_int, name: &str) -> Result<Self> {
        let pattern = x11::XftNameParse(name)?;

        let matched = x11::XftFontMatch(dpy, scr, pattern)?;
        let font = x11::XftFontOpenPattern(dpy, matched)?;
        x11::FcPatternDestroy(matched);
        let extents = x11::XftTextExtentsUtf8(dpy, font, ASCII_PRINTABLE);

        let height = x11::font_ascent(font) + x11::font_descent(font);
        let len = ASCII_PRINTABLE.len();

        // Divceil (round the width up).
        let width = (extents.xOff as usize + (len - 1)) / len;

        let slant = CString::new("slant").unwrap();
        let weight = CString::new("weight").unwrap();

        x11::FcPatternDel(pattern, &slant);
        x11::FcPatternAddInteger(pattern, &slant, x11::FC_SLANT_ITALIC);
        let matched = x11::XftFontMatch(dpy, scr, pattern)?;
        let ifont = x11::XftFontOpenPattern(dpy, matched)?;
        x11::FcPatternDestroy(matched);

        x11::FcPatternDel(pattern, &weight);
        x11::FcPatternAddInteger(pattern, &weight, x11::FC_WEIGHT_BOLD);
        let matched = x11::XftFontMatch(dpy, scr, pattern)?;
        let ibfont = x11::XftFontOpenPattern(dpy, matched)?;
        x11::FcPatternDestroy(matched);

        x11::FcPatternDel(pattern, &slant);
        x11::FcPatternAddInteger(pattern, &slant, x11::FC_SLANT_ROMAN);
        let matched = x11::XftFontMatch(dpy, scr, pattern)?;
        let bfont = x11::XftFontOpenPattern(dpy, matched)?;
        x11::FcPatternDestroy(matched);

        x11::FcPatternDestroy(pattern);
        Ok(Self {
            height,
            width,
            font,
            bfont,
            ifont,
            ibfont,
        })
    }

    pub fn get(&self, attr: GlyphAttr) -> x11::XftFont {
        if attr.contains(GlyphAttr::BOLD | GlyphAttr::ITALIC) {
            return self.ibfont;
        }
        if attr.contains(GlyphAttr::BOLD) {
            return self.bfont;
        }
        if attr.contains(GlyphAttr::ITALIC) {
            return self.ifont;
        }
        self.font
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn ascent(&self) -> usize {
        x11::font_ascent(self.font)
    }
}
