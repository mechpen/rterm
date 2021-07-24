use crate::charset::{Charset, CharsetIndex};
use crate::glyph::GlyphAttr;
use crate::term::{Term, TermMode};
use crate::win::{Win, WinMode};
use std::iter;
use vte::{Params, Parser, Perform};

pub struct Vte {
    parser: Parser,
    last_c: Option<char>,
}

impl Vte {
    pub fn new() -> Self {
        Vte {
            parser: Parser::new(),
            last_c: None,
        }
    }

    pub fn process_input(&mut self, buf: &[u8], win: &mut Win, term: &mut Term) {
        let mut performer = Performer::new(win, term, self.last_c.take());
        buf.iter()
            .for_each(|&b| self.parser.advance(&mut performer, b));
        self.last_c = performer.last_c.take();
    }
}

const VTIDEN: &[u8] = b"\x1B[?6c";

struct Performer<'a> {
    win: &'a mut Win,
    term: &'a mut Term,
    last_c: Option<char>,
}

impl<'a> Performer<'a> {
    pub fn new(win: &'a mut Win, term: &'a mut Term, last_c: Option<char>) -> Self {
        Self { win, term, last_c }
    }

    fn set_glyph_attr(&mut self, params: &Params) {
        let prop = &mut self.term.c.glyph.prop;

        if params.is_empty() {
            prop.reset();
            return;
        }

        for param in params.iter() {
            match param[0] {
                0 => prop.reset(),
                1 => prop.attr.insert(GlyphAttr::BOLD),
                2 => prop.attr.insert(GlyphAttr::FAINT),
                3 => prop.attr.insert(GlyphAttr::ITALIC),
                4 => prop.attr.insert(GlyphAttr::UNDERLINE),
                5 | 6 => prop.attr.insert(GlyphAttr::BLINK),
                7 => prop.attr.insert(GlyphAttr::REVERSE),
                8 => prop.attr.insert(GlyphAttr::INVISIBLE),
                9 => prop.attr.insert(GlyphAttr::STRUCK),
                22 => prop.attr.remove(GlyphAttr::BOLD_FAINT),
                23 => prop.attr.remove(GlyphAttr::ITALIC),
                24 => prop.attr.remove(GlyphAttr::UNDERLINE),
                25 => prop.attr.remove(GlyphAttr::BLINK),
                27 => prop.attr.remove(GlyphAttr::REVERSE),
                28 => prop.attr.remove(GlyphAttr::INVISIBLE),
                29 => prop.attr.remove(GlyphAttr::STRUCK),
                30..=37 => prop.fg = (param[0] - 30) as usize,
                38 => (), // FIXME
                39 => prop.reset_fg(),
                40..=47 => prop.bg = (param[0] - 40) as usize,
                48 => (), // FIXME
                49 => prop.reset_bg(),
                90..=97 => prop.fg = (param[0] - 90 + 8) as usize,
                100..=107 => prop.bg = (param[0] - 100 + 8) as usize,
                _ => println!("unknown glyph attr {}", param[0]),
            }
        }
    }

    fn set_mode(&mut self, intermediate: Option<&u8>, params: &Params, val: bool) {
        let private = match intermediate {
            Some(b'?') => true,
            None => false,
            _ => return,
        };

        if private {
            for param in params.iter() {
                match param[0] {
                    1 =>
                    // DECCKM -- Cursor key
                    {
                        self.win.set_mode(WinMode::APPCURSOR, val)
                    }
                    5 =>
                    // DECSCNM -- Reverse video
                    {
                        self.win.set_mode(WinMode::REVERSE, val)
                    }
                    6 =>
                    // DECOM -- Origin
                    {
                        self.term.set_mode(TermMode::ORIGIN, val);
                        self.term.move_ato(0, 0);
                    }
                    7 =>
                    // DECAWM -- Auto wrap
                    {
                        self.term.set_mode(TermMode::WRAP, val)
                    }
                    25 =>
                    // DECTCEM -- Text Cursor Enable Mode
                    {
                        self.win.set_mode(WinMode::HIDE, !val)
                    }
                    1034 => self.win.set_mode(WinMode::EIGHT_BIT, val),
                    1048 => {
                        if val {
                            self.term.save_cursor();
                        } else {
                            self.term.load_cursor();
                        }
                    }
                    _ => (),
                }
            }
        } else {
            for param in params.iter() {
                match param[0] {
                    4 =>
                    // IRM -- Insertion-replacement
                    {
                        self.term.set_mode(TermMode::INSERT, val)
                    }
                    12 =>
                    // SRM -- Send/Receive
                    {
                        self.win.set_mode(WinMode::ECHO, !val)
                    }
                    20 =>
                    // LNM -- Linefeed/new line
                    {
                        self.term.set_mode(TermMode::CRLF, val)
                    }
                    _ => (),
                }
            }
        }
    }
}

impl<'a> Perform for Performer<'a> {
    fn print(&mut self, c: char) {
        let c = self.term.charset.map(c);
        self.term.put_char(c);
        self.last_c = Some(c);
    }

    fn execute(&mut self, byte: u8) {
        let win = &mut *self.win;
        let term = &mut *self.term;

        match byte {
            0x07 =>
            // BEL
            {
                win.bell()
            }
            0x08 =>
            // BS
            {
                term.move_to(term.c.x.saturating_sub(1), term.c.y)
            }
            0x09 =>
            // HT
            {
                term.put_tabs(1)
            }
            0x0D =>
            // CR
            {
                term.move_to(0, term.c.y)
            }
            0x0A | 0x0B | 0x0C =>
            // LF VT FF
            {
                term.new_line(false)
            }
            0x0E =>
            // SO
            {
                term.charset.set_current(CharsetIndex::G1)
            }
            0x0F =>
            // SI
            {
                term.charset.set_current(CharsetIndex::G0)
            }
            _ => println!("unknown control {:02x}", byte),
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        let win = &mut *self.win;
        let term = &mut *self.term;
        let intermediate = intermediates.get(0);

        match (byte, intermediate) {
            (b'B', Some(b'(')) => term.charset.setup(CharsetIndex::G0, Charset::Ascii),
            (b'B', Some(b')')) => term.charset.setup(CharsetIndex::G1, Charset::Ascii),
            (b'B', Some(b'*')) => term.charset.setup(CharsetIndex::G2, Charset::Ascii),
            (b'B', Some(b'+')) => term.charset.setup(CharsetIndex::G3, Charset::Ascii),
            (b'D', None) =>
            // IND -- Linefeed
            {
                term.new_line(false)
            }
            (b'E', None) =>
            // NEL -- Next line
            {
                term.new_line(true)
            }
            (b'H', None) =>
            // HTS -- Horizontal tab stop
            {
                term.set_tab(term.c.x)
            }
            (b'M', None) =>
            // RI -- Reverse index
            {
                if term.c.y == term.scroll_top {
                    term.scroll_down(term.scroll_top, 1);
                } else {
                    term.move_to(term.c.x, term.c.y - 1);
                }
            }
            (b'Z', None) =>
            // DECID -- Identify Terminal
            {
                term.pty.write(VTIDEN.to_vec())
            }
            (b'c', None) =>
            // RIS -- Reset to initial state
            {
                term.reset()
            } // FIXME: reset title and etc.
            (b'0', Some(b'(')) => term.charset.setup(CharsetIndex::G0, Charset::Graphic0),
            (b'0', Some(b')')) => term.charset.setup(CharsetIndex::G1, Charset::Graphic0),
            (b'0', Some(b'*')) => term.charset.setup(CharsetIndex::G2, Charset::Graphic0),
            (b'0', Some(b'+')) => term.charset.setup(CharsetIndex::G3, Charset::Graphic0),
            (b'7', None) =>
            // DECSC -- Save Cursor
            {
                term.save_cursor()
            }
            (b'8', None) =>
            // DECRC -- Restore Cursor
            {
                term.load_cursor()
            }
            (b'=', None) =>
            // DECPAM -- Application keypad
            {
                win.set_mode(WinMode::APPKEYPAD, true)
            }
            (b'>', None) =>
            // DECPNM -- Normal keypad
            {
                win.set_mode(WinMode::APPKEYPAD, false)
            }
            (b'\\', None) =>
                // ST -- String Terminator
                {}
            _ => println!("unknown esc {:?} {}", intermediate, byte as char),
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        has_ignored_intermediates: bool,
        action: char,
    ) {
        let term = &mut *self.term;

        if has_ignored_intermediates || intermediates.len() > 1 {
            println!("invalid csi intermediates {:?}", intermediates);
            return;
        }
        let intermediate = intermediates.get(0);

        let mut params_iter = params.iter();
        let arg0 = params_iter.next();
        let arg1 = params_iter.next();
        let arg0_or = |default: usize| {
            arg0.map(|x| x[0] as usize)
                .filter(|&x| x != 0)
                .unwrap_or(default)
        };
        let arg1_or = |default: usize| {
            arg1.map(|x| x[0] as usize)
                .filter(|&x| x != 0)
                .unwrap_or(default)
        };

        match (action, intermediate) {
            ('@', None) => // ICH -- Insert <n> blank char
                term.insert_blanks(arg0_or(1)),
            ('A', None) => // CUU -- Cursor <n> Up
                term.move_to(term.c.x, term.c.y.saturating_sub(arg0_or(1))),
            ('B', None) |  // CUD -- Cursor <n> Down
            ('e', None) => // VPR -- Cursor <n> Down
                term.move_to(term.c.x, term.c.y+arg0_or(1)),
            ('b', None) => // REP -- Print last char <n> times
            {
                if let Some(c) = self.last_c {
                    iter::repeat(c)
                        .take(arg0_or(1))
                        .for_each(|c| term.put_char(c));
                }
            },
            ('C', None) |  // CUF -- Cursor <n> Forward
            ('a', None) => // HPR -- Cursor <n> Forward
                term.move_to(term.c.x+arg0_or(1), term.c.y),
            ('c', _) if arg0_or(0) == 0 => // DA -- Device Attributes
                term.pty.write(VTIDEN.to_vec()),
            ('D', None) => // CUB -- Cursor <n> Backward
                term.move_to(term.c.x.saturating_sub(arg0_or(1)), term.c.y),
            ('d', None) => // VPA -- Move to <row>
                term.move_ato(term.c.x, arg0_or(1)-1),
            ('E', None) => // CNL -- Cursor <n> Down and first col
                term.move_to(0, term.c.y+arg0_or(1)),
            ('F', None) => // CPL -- Cursor <n> Up and first col
                term.move_to(0, term.c.y.saturating_sub(arg0_or(1))),
            ('G', None) |  // CHA -- Move to <col>
            ('`', None) => // HPA
                term.move_to(arg0_or(1)-1, term.c.y),
            ('g', None) => // TBC -- Tabulation clear
            {
                match arg0_or(0) {
                    0 => // clear current tab stop
                        term.clear_tabs(iter::once(term.c.x)),
                    3 => // clear all the tabs
                        term.clear_tabs(0..term.cols),
                    x => println!("unknown TBC {}", x),
                }
            },
            ('H', None) |  // CUP -- Move to <row> <col>
            ('f', None) => // HVP
                term.move_ato(arg1_or(1)-1, arg0_or(1)-1),
            ('h', intermediate) => // SM -- Set terminal mode
                self.set_mode(intermediate, params, true),
            ('I', None) => // CHT -- Cursor Forward Tabulation <n> tab stops
                term.put_tabs(arg0_or(1) as i32),
            ('J', None) => // ED -- Clear screen
            {
                let y = term.c.y;
                match arg0_or(0) {
                    0 => // below
                    {
                        term.clear_region(term.c.x..term.cols, iter::once(y));
                        term.clear_region(0..term.cols, y+1..term.rows);
                    },
                    1 => // above
                    {
                        term.clear_region(0..term.cols, 0..y);
                        term.clear_region(0..=term.c.x, iter::once(y));
                    },
                    2 => // all
                        term.clear_region(0..term.cols, 0..term.rows),
                    x => println!("unknown ED {}", x),
                }
            },
            ('K', None) => // EL erase line
            {
                let y = term.c.y;
                match arg0_or(0) {
                    0 => // right
                        term.clear_region(term.c.x..term.cols, iter::once(y)),
                    1 => // left
                        term.clear_region(0..=term.c.x, iter::once(y)),
                    2 => // all
                        term.clear_region(0..term.cols, iter::once(y)),
                    x => println!("unknown EL {}", x),
                }
            },
            ('L', None) => // IL -- Insert <n> blank lines
                term.insert_lines(arg0_or(1)),
            ('l', intermediate) => // RM -- Reset Mode
                self.set_mode(intermediate, params, false),
            ('M', None) => // DL -- Delete <n> lines
                term.delete_lines(arg0_or(1)),
            ('m', None) => // SGR -- Terminal attribute (color)
                self.set_glyph_attr(params),
            ('n', None) if arg0_or(0) == 6 => // DSR Device Status Report
            {
                let s = format!("\x1B[{};{}R", term.c.y+1, term.c.x+1);
                term.pty.write(s.as_bytes().to_vec());
            }
            ('P', None) => // DCH -- Delete <n> char
                term.delete_chars(arg0_or(1)),
            ('r', None) => // DECSTBM -- Set Scrolling Region
            {
                let top = arg0_or(1) - 1;
                let bot = arg1_or(term.rows) - 1;
                term.set_scroll(top, bot);
                term.move_ato(0, 0);
            },
            ('S', None) => // SU -- Scroll <n> line up
                term.scroll_up(term.scroll_top, arg0_or(1)),
            ('s', None) => // DECSC -- Save cursor position (ANSI.SYS)
                term.save_cursor(),
            ('T', None) => // SD -- Scroll <n> line down
                term.scroll_down(term.scroll_top, arg0_or(1)),
            ('u', None) => // DECRC -- Restore cursor position (ANSI.SYS)
                term.load_cursor(),
            ('X', None) => // ECH -- Erase <n> char
                term.clear_region(
                    term.c.x..term.c.x+arg0_or(1), iter::once(term.c.y)
                ),
            ('Z', None) => // CBT -- Cursor Backward Tabulation <n> tab stops
                term.put_tabs(-(arg0_or(1) as i32)),
            _ => println!(
                "unknown csi {:?} {:?} {}", intermediates, params, action
            ),
        }
    }
}
