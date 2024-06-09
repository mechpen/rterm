// control sequence references:
// - https://vt100.net/
// - https://invisible-island.net/xterm/ctlseqs/ctlseqs.html

use crate::charset::{Charset, CharsetIndex};
use crate::color::{
    BG_COLOR, BG_COLOR_NAME,
    FG_COLOR, FG_COLOR_NAME,
    CURSOR_COLOR, CURSOR_COLOR_NAME,
};
use crate::cursor::CursorMode;
use crate::glyph::GlyphAttr;
use crate::pty::Pty;
use crate::term::{Term, TermMode};
use crate::win::{Win, WinMode};

use std::iter;

use vte::{Params, ParamsIter, Parser, Perform};

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const VTIDEN: &[u8] = b"\x1B[?6c";

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

    pub fn process_input(
        &mut self, buf: &[u8], win: &mut Win, term: &mut Term, pty: &mut Pty
    ) {
        let mut performer = Performer::new(win, term, pty, self.last_c.take());
        buf.iter().for_each(|&b| self.parser.advance(&mut performer, b));
        self.last_c = performer.last_c.take();
    }
}

struct Performer<'a> {
    win: &'a mut Win,
    term: &'a mut Term,
    pty: &'a mut Pty,
    last_c: Option<char>,
}

impl<'a> Performer<'a> {
    pub fn new(
        win: &'a mut Win,
        term: &'a mut Term,
        pty: &'a mut Pty,
        last_c: Option<char>,
    ) -> Self {
        Self {
            win,
            term,
            pty,
            last_c,
        }
    }

    fn defcolor(params: &mut ParamsIter) -> usize {
        fn getcolor(params: &mut ParamsIter) -> Option<u16> {
            if let Some(col) = params.next() {
                if col.len() == 1 {
                    Some(col[0])
                } else {
                    None
                }
            } else {
                None
            }
        }

        let mut color: usize = 0;
        if let Some(op) = params.next() {
            if op.is_empty() {
                return 0;
            }
            match op[0] {
                // direct color in RGB space
                2 => {
                    let r = getcolor(params);
                    let g = getcolor(params);
                    let b = getcolor(params);
                    if let (Some(r), Some(g), Some(b)) = (r, g, b) {
                        if r > 255 || g > 255 || b > 255 {
                            println!("bad rgb color ({},{},{})\n", r, g, b);
                        } else {
                            color = 1 << 24
                                | (r as usize) << 16
                                | (g as usize) << 8
                                | b as usize;
                        }
                    } else {
                        println!("Incorrect number of rgb parameters");
                    }
                }
                // indexed color
                5 => {
                    if let Some(c) = getcolor(params) {
                        if c <= 255 {
                            color = c as usize;
                        } else {
                            println!("Incorrect color index: {}", c);
                        }
                    } else {
                        println!("Missing color index parameter");
                    }
                }
                0 => {} // implemented defined (only foreground)
                1 => {} // transparent
                3 => {} // direct color in CMY space
                4 => {} // direct color in CMYK space
                x => {
                    println!("gfx attr {} unknown\n", x);
                }
            }
        } else {
            println!("unknown color glyph attr");
        }
        color
    }

    fn set_glyph_attr(&mut self, params: &Params) {
        let prop = &mut self.term.prop;

        if params.is_empty() {
            prop.reset();
            return;
        }

        let mut params = params.iter();
        while let Some(param) = params.next() {
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
                38 => prop.fg = Self::defcolor(&mut params),
                39 => prop.reset_fg(),
                40..=47 => prop.bg = (param[0] - 40) as usize,
                48 => prop.bg = Self::defcolor(&mut params),
                49 => prop.reset_bg(),
                90..=97 => prop.fg = (param[0] - 90 + 8) as usize,
                100..=107 => prop.bg = (param[0] - 100 + 8) as usize,
                _ => println!("unknown glyph attr {}", param[0]),
            }
        }
    }

    fn set_mode(
        &mut self, intermediate: Option<&u8>, params: &Params, val: bool
    ) {
        let private = match intermediate {
            Some(b'?') => true,
            None => false,
            _ => return,
        };

        if private {
            for param in params.iter() {
                match param[0] {
                    // DECCKM -- Cursor key
                    1 => self.win.set_mode(WinMode::APPCURSOR, val),
                    // DECSCNM -- Reverse video
                    5 => self.win.set_mode(WinMode::REVERSE, val),
                    // DECOM -- Origin
                    6 => {
                        self.term.set_mode(TermMode::ORIGIN, val);
                        self.term.move_ato(0, 0);
                    }
                    // DECAWM -- Auto wrap
                    7 => self.term.set_mode(TermMode::WRAP, val),
                    // DECTCEM -- Text Cursor Enable Mode
                    25 => self.win.set_mode(WinMode::HIDE, !val),
                    // X10 mouse compatibility mode
                    9 => {
                        self.win.set_pointer_motion(false);
                        self.win.set_mode(WinMode::MOUSE, false);
                        self.win.set_mode(WinMode::MOUSEX10, val);
                    }
                    // 1000: report button press
                    1000 => {
                        self.win.set_pointer_motion(false);
                        self.win.set_mode(WinMode::MOUSE, false);
                        self.win.set_mode(WinMode::MOUSEBTN, val);
                    }
                    // 1002: report motion on button press
                    1002 => {
                        self.win.set_pointer_motion(false);
                        self.win.set_mode(WinMode::MOUSE, false);
                        self.win.set_mode(WinMode::MOUSEMOTION, val);
                    }
                    // 1003: enable all mouse motions
                    1003 => {
                        self.win.set_pointer_motion(val);
                        self.win.set_mode(WinMode::MOUSE, false);
                        self.win.set_mode(WinMode::MOUSEMANY, val);
                    }
                    // 1004: send focus events to tty
                    1004 => self.win.set_mode(WinMode::FOCUS, val),
                    // 1006: extended reporting mode
                    1006 => self.win.set_mode(WinMode::MOUSESGR, val),
                    1034 => self.win.set_mode(WinMode::EIGHT_BIT, val),
                    // 1048: save/load cursor position
                    1048 => {
                        if val {
                            self.term.save_cursor();
                        } else {
                            self.term.load_cursor();
                        }
                    }
                    // 47: swap screen
                    47 => self.term.swap_screen(val),
                    // 1047: swap screen and clear alt
                    1047 => {
                        if val {
                            self.term.swap_screen(val);
                        } else {
                            self.term.clear_screen();
                            self.term.swap_screen(val);
                        }
                    }
                    // 1049: save/load cursor, swap screen and clear alt
                    1049 => {
                        if val {
                            self.term.save_cursor();
                            self.term.swap_screen(val);
                            self.term.clear_screen();
                        } else {
                            self.term.clear_screen();
                            self.term.swap_screen(val);
                            self.term.load_cursor();
                        }
                    }
                    _ => (),
                }
            }
        } else {
            for param in params.iter() {
                match param[0] {
                    // IRM -- Insertion-replacement
                    4 => self.term.set_mode(TermMode::INSERT, val),
                    // SRM -- Send/Receive
                    12 => self.win.set_mode(WinMode::ECHO, !val),
                    // LNM -- Linefeed/new line
                    20 => self.term.set_mode(TermMode::CRLF, val),
                    _ => (),
                }
            }
        }
    }

    fn send_color_osc(
        &mut self, idx: usize, leader: &str, bell_terminated: bool
    ) {
        let mut v: Vec<u8> = vec![0x1b];
        v.extend_from_slice(
            format!("]{};{}", leader, self.win.get_color_osc(idx).unwrap())
                .as_bytes(),
        );
        if bell_terminated {
            v.push(0x07);
        } else {
            v.push(0x1b);
            v.push(b'\\');
        }
        self.pty.write(&v);
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
            // BEL
            0x07 => win.bell(),
            // BS
            0x08 => term.move_to(term.c.x.saturating_sub(1), term.c.y),
            // HT
            0x09 => term.put_tabs(1),
            // CR
            0x0D => term.move_to(0, term.c.y),
            // LF VT FF
            0x0A | 0x0B | 0x0C => term.new_line(false),
            // SO
            0x0E => term.charset.set_current(CharsetIndex::G1),
            // SI
            0x0F => term.charset.set_current(CharsetIndex::G0),
            _ => println!("unknown control {:02x}", byte),
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        let win = &mut *self.win;
        let term = &mut *self.term;
        let intermediate = intermediates.get(0);

        match (byte, intermediate) {
            (b'B', Some(b'(')) =>
                term.charset.setup(CharsetIndex::G0, Charset::Ascii),
            (b'B', Some(b')')) =>
                term.charset.setup(CharsetIndex::G1, Charset::Ascii),
            (b'B', Some(b'*')) =>
                term.charset.setup(CharsetIndex::G2, Charset::Ascii),
            (b'B', Some(b'+')) =>
                term.charset.setup(CharsetIndex::G3, Charset::Ascii),
            // IND -- Linefeed
            (b'D', None) => term.new_line(false),
            // NEL -- Next line
            (b'E', None) => term.new_line(true),
            // HTS -- Horizontal tab stop
            (b'H', None) => term.set_tab(term.c.x),
            // RI -- Reverse index
            (b'M', None) => {
                if term.c.y == term.scroll_top {
                    term.scroll_down(term.scroll_top, 1);
                } else {
                    term.move_to(term.c.x, term.c.y - 1);
                }
            }
            // DECID -- Identify Terminal
            (b'Z', None) => self.pty.write(VTIDEN),
            // RIS -- Reset to initial state
            (b'c', None) => {
                win.reset_colors();
                term.reset()
                // FIXME: reset title and etc.
            }
            (b'0', Some(b'(')) =>
                term.charset.setup(CharsetIndex::G0, Charset::Graphic0),
            (b'0', Some(b')')) =>
                term.charset.setup(CharsetIndex::G1, Charset::Graphic0),
            (b'0', Some(b'*')) =>
                term.charset.setup(CharsetIndex::G2, Charset::Graphic0),
            (b'0', Some(b'+')) =>
                term.charset.setup(CharsetIndex::G3, Charset::Graphic0),
            // DECSC -- Save Cursor
            (b'7', None) => term.save_cursor(),
            // DECRC -- Restore Cursor
            (b'8', None) => term.load_cursor(),
            // DECPAM -- Application keypad
            (b'=', None) => win.set_mode(WinMode::APPKEYPAD, true),
            // DECPNM -- Normal keypad
            (b'>', None) => win.set_mode(WinMode::APPKEYPAD, false),
            // ST -- String Terminator
            (b'\\', None) => {}
            _ => println!("unknown esc {:?} {}", intermediate, byte as char),
        }
    }

    // FIXME: styling
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        match params[0] {
            b"0" => {
                if let Some(title) = params.get(1) {
                    self.win.settitle(&String::from_utf8_lossy(title));
                    self.win.seticontitle(&String::from_utf8_lossy(title));
                }
            }
            b"1" => {
                if let Some(title) = params.get(1) {
                    self.win.seticontitle(&String::from_utf8_lossy(title));
                }
            }
            b"2" => {
                if let Some(title) = params.get(1) {
                    self.win.settitle(&String::from_utf8_lossy(title));
                }
            }
            b"52" => {} // FIXME
            b"4" => {
                // color set, color index;spec
                let mut params = params.iter();
                params.next(); // skip the param "4"
                while let Some(col_idx) = params.next() {
                    if let Some(col_name) = params.next() {
                        if let Ok(idx) = String::from_utf8_lossy(col_idx).parse::<u16>() {
                            if (idx as usize) < self.win.num_colors() {
                                let name = String::from_utf8_lossy(col_name);
                                if name == "?" {
                                    self.send_color_osc(
                                        idx as usize,
                                        &format!("4;{}", idx),
                                        bell_terminated,
                                    );
                                } else if let Err(err) = self.win.setcolor(idx, Some(&name)) {
                                    println!("OSC 4 error: {}", err);
                                }
                            } else {
                                println!(
                                    "OSC 4, color index to large, max is {}",
                                    self.win.num_colors()
                                );
                            }
                        } else {
                            println!("OSC 4, color index not a number");
                        }
                    } else {
                        println!("OSC 4, missing color name");
                        break;
                    }
                }
                self.win.redraw(&mut self.term);
            }
            b"10" => {
                /* set foreground color */
                let mut params = params.iter();
                params.next(); // skip the consumed param
                if let Some(col_name) = params.next() {
                    if params.next().is_none() {
                        let name = String::from_utf8_lossy(col_name);
                        if name == "?" {
                            self.send_color_osc(FG_COLOR, "10", bell_terminated);
                        } else if let Err(err) = self.win.setcolor(FG_COLOR as u16, Some(&name)) {
                            println!("OSC 10 error: {}", err);
                        }
                    } else {
                        println!("OSC 10, to many parameters");
                    }
                } else {
                    println!("OSC 10, missing color name");
                }
                self.win.redraw(&mut self.term);
            }
            b"11" => {
                /* set background color */
                let mut params = params.iter();
                params.next(); // skip the consumed param
                if let Some(col_name) = params.next() {
                    if params.next().is_none() {
                        let name = String::from_utf8_lossy(col_name);
                        if name == "?" {
                            self.send_color_osc(BG_COLOR, "11", bell_terminated);
                        } else if let Err(err) = self.win.setcolor(BG_COLOR as u16, Some(&name)) {
                            println!("OSC 11 error: {}", err);
                        }
                    } else {
                        println!("OSC 11, to many parameters");
                    }
                } else {
                    println!("OSC 11, missing color name");
                }
                self.win.redraw(&mut self.term);
            }
            b"12" => {
                /* set cursor color */
                let mut params = params.iter();
                params.next(); // skip the consumed param
                if let Some(col_name) = params.next() {
                    if params.next().is_none() {
                        let name = String::from_utf8_lossy(col_name);
                        if name == "?" {
                            self.send_color_osc(CURSOR_COLOR, "12", bell_terminated);
                        } else if let Err(err) = self.win.setcolor(CURSOR_COLOR as u16, Some(&name))
                        {
                            println!("OSC 12 error: {}", err);
                        }
                    } else {
                        println!("OSC 12, to many parameters");
                    }
                } else {
                    println!("OSC 12, missing color name");
                }
                self.win.redraw(&mut self.term);
            }
            b"104" => {
                /* color reset, optional color index */
                let mut params = params.iter();
                params.next(); // skip the param "104"
                let mut cnt = 0;
                for col_idx in params {
                    cnt += 1;
                    if let Ok(idx) = String::from_utf8_lossy(col_idx).parse::<u16>() {
                        if (idx as usize) < self.win.num_colors() {
                            if let Err(err) = self.win.setcolor(idx, None) {
                                println!("OSC 104 error: {}", err);
                            }
                        } else {
                            println!(
                                "OSC 104, color index to large, max is {}",
                                self.win.num_colors()
                            );
                        }
                    } else {
                        println!("OSC 104, color index not a number");
                    }
                }
                if cnt == 0 {
                    self.win.reset_colors();
                }
                self.win.redraw(&mut self.term);
            }
            b"110" => {
                /* foreground color reset */
                let mut params = params.iter();
                params.next(); // skip the consumed param
                if params.next().is_none() {
                    if let Err(err) = self.win.setcolor(FG_COLOR as u16, Some(FG_COLOR_NAME)) {
                        println!("OSC 110 error: {}", err);
                    }
                } else {
                    println!("OSC 110 takes no parameters");
                }
                self.win.redraw(&mut self.term);
            }
            b"111" => {
                /* background color reset */
                let mut params = params.iter();
                params.next(); // skip the consumed param
                if params.next().is_none() {
                    if let Err(err) = self.win.setcolor(BG_COLOR as u16, Some(BG_COLOR_NAME)) {
                        println!("OSC 111 error: {}", err);
                    }
                } else {
                    println!("OSC 111 takes no parameters");
                }
                self.win.redraw(&mut self.term);
            }
            b"112" => {
                /* cursor color reset */
                let mut params = params.iter();
                params.next(); // skip the consumed param
                if params.next().is_none() {
                    if let Err(err) = self
                        .win
                        .setcolor(CURSOR_COLOR as u16, Some(CURSOR_COLOR_NAME))
                    {
                        println!("OSC 112 error: {}", err);
                    }
                } else {
                    println!("OSC 112 takes no parameters");
                }
                self.win.redraw(&mut self.term);
            }
            _ => {
                println!("Unknown OSC command {}", String::from_utf8_lossy(params[0]));
            }
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
        let (x, y) = (term.c.x, term.c.y);

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
            // ICH -- Insert <n> blank char
            ('@', None) => term.insert_blanks(arg0_or(1)),
            // CUU -- Cursor <n> Up
            ('A', None) => term.move_to(x, y.saturating_sub(arg0_or(1))),
            // CUD -- Cursor <n> Down
            // VPR -- Cursor <n> Down
            ('B', None) | ('e', None) => term.move_to(x, y + arg0_or(1)),
            // REP -- Print last char <n> times
            ('b', None) => {
                if let Some(c) = self.last_c {
                    iter::repeat(c)
                        .take(arg0_or(1))
                        .for_each(|c| term.put_char(c));
                }
            }
            // CUF -- Cursor <n> Forward | HPR -- Cursor <n> Forward
            ('C', None) | ('a', None) => term.move_to(x + arg0_or(1), y),
            // DA -- Device Attributes
            ('c', None) if arg0_or(0) == 0 => self.pty.write(VTIDEN),
            // DA2 -- Secondary Device Attributes
            ('c', Some(b'>')) if arg0_or(0) == 0 => {} // FIXME
            // CUB -- Cursor <n> Backward
            ('D', None) => term.move_to(x.saturating_sub(arg0_or(1)), y),
            // VPA -- Move to <row>
            ('d', None) => term.move_ato(x, arg0_or(1) - 1),
            // CNL -- Cursor <n> Down and first col
            ('E', None) => term.move_to(0, y + arg0_or(1)),
            // CPL -- Cursor <n> Up and first col
            ('F', None) => term.move_to(0, y.saturating_sub(arg0_or(1))),
            // CHA -- Move to <col> | HPA
            ('G', None) | ('`', None) => term.move_to(arg0_or(1) - 1, y),
            // TBC -- Tabulation clear
            ('g', None) => match arg0_or(0) {
                // clear current tab stop
                0 => term.clear_tabs(iter::once(x)),
                // clear all the tabs
                3 => term.clear_tabs(0..term.cols),
                v => println!("unknown TBC {}", v),
            }
            // CUP -- Move to <row> <col> |  HVP
            ('H', None) | ('f', None) =>
                term.move_ato(arg1_or(1) - 1, arg0_or(1) - 1),
            // SM -- Set terminal mode
            ('h', intermediate) => self.set_mode(intermediate, params, true),
            // CHT -- Cursor Forward Tabulation <n> tab stops
            ('I', None) => term.put_tabs(arg0_or(1) as i32),
            // ED -- Clear screen
            ('J', None) => match arg0_or(0) {
                // below
                0 => {
                    term.clear_region(x..term.cols, iter::once(y));
                    term.clear_region(0..term.cols, y + 1..term.rows);
                }
                // above
                1 => {
                    term.clear_region(0..term.cols, 0..y);
                    term.clear_region(0..=x, iter::once(y));
                }
                // all
                2 => term.clear_screen(),
                v => println!("unknown ED {}", v),
            }
            // EL erase line
            ('K', None) => match arg0_or(0) {
                // right
                0 => term.clear_region(x..term.cols, iter::once(y)),
                // left
                1 => term.clear_region(0..=x, iter::once(y)),
                // all
                2 => term.clear_region(0..term.cols, iter::once(y)),
                v => println!("unknown EL {}", v),
            }
            // IL -- Insert <n> blank lines
            ('L', None) => term.insert_lines(arg0_or(1)),
            // RM -- Reset Mode
            ('l', intermediate) => self.set_mode(intermediate, params, false),
            // DL -- Delete <n> lines
            ('M', None) => term.delete_lines(arg0_or(1)),
            // SGR -- Terminal attribute (color)
            ('m', None) => self.set_glyph_attr(params),
            // DSR Device Status Report
            ('n', None) if arg0_or(0) == 6 =>
                self.pty.write(format!("\x1B[{};{}R", y + 1, x + 1).as_bytes()),
            // DCH -- Delete <n> char
            ('P', None) => term.delete_chars(arg0_or(1)),
            // DECSTBM -- Set Scrolling Region
            ('r', None) => {
                let top = arg0_or(1) - 1;
                let bot = arg1_or(term.rows) - 1;
                term.set_scroll(top, bot);
                term.move_ato(0, 0);
            }
            // SU -- Scroll <n> line up
            ('S', None) => term.scroll_up(term.scroll_top, arg0_or(1)),
            // DECSC -- Save cursor position (ANSI.SYS)
            ('s', None) => term.save_cursor(),
            // SD -- Scroll <n> line down
            ('T', None) => term.scroll_down(term.scroll_top, arg0_or(1)),
            // DECRC -- Restore cursor position (ANSI.SYS)
            ('u', None) => term.load_cursor(),
            // ECH -- Erase <n> char
            ('X', None) => term.clear_region(x..x + arg0_or(1), iter::once(y)),
            // CBT -- Cursor Backward Tabulation <n> tab stops
            ('Z', None) => term.put_tabs(-(arg0_or(1) as i32)),
            // XTVERSION -- Return the terminal name/version
            ('q', Some(b'>')) if arg0_or(0) == 0 => {
                let s = format!("\x1bP>|{} {}\x1b\\", NAME, VERSION);
                self.pty.write(s.as_bytes());
            }
            // DECSLPP
            ('t', None) => match arg0_or(0) {
                22 => {
                    if let Some(title) = self.win.title() {
                        self.term.push_title(title);
                    }
                }
                23 => {
                    if let Some(title) = self.term.pop_title() {
                        self.win.settitle(&title);
                    }
                }
                _ => (),
            },
            // DECSCUSR -- Set Cursor Style
            ('q', Some(b' ')) => match arg0_or(0) {
                // Blinking block
                0 | 1 => {
                    term.c.mode = CursorMode::Block;
                    term.c.blink = true;
                }
                // block
                2 => {
                    term.c.mode = CursorMode::Block;
                    term.c.blink = false;
                }
                // blinking underline
                3 => {
                    term.c.mode = CursorMode::Underline;
                    term.c.blink = true;
                }
                // underline
                4 => {
                    term.c.mode = CursorMode::Underline;
                    term.c.blink = false;
                }
                // blinking bar
                5 => {
                    term.c.mode = CursorMode::Bar;
                    term.c.blink = true;
                }
                // bar
                6 => {
                    term.c.mode = CursorMode::Bar;
                    term.c.blink = false;
                }
                v => println!("unknown cursor style {}", v),
            }
            _ => println!(
                "unknown csi {:?} {:?} {}", intermediates, params, action
            ),
        }
    }
}
