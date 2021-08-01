use crate::charset::{Charset, CharsetIndex};
use crate::color::{BG_COLOR, BG_COLOR_NAME, FG_COLOR, FG_COLOR_NAME};
use crate::glyph::GlyphAttr;
use crate::term::{Term, TermMode};
use crate::win::{Win, WinMode};
use std::iter;
use vte::{Params, ParamsIter, Parser, Perform};

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
                2 => {
                    /* direct color in RGB space */
                    let r = getcolor(params);
                    let g = getcolor(params);
                    let b = getcolor(params);
                    if let (Some(r), Some(g), Some(b)) = (r, g, b) {
                        if r > 255 || g > 255 || b > 255 {
                            println!("erresc: bad rgb color ({},{},{})\n", r, g, b);
                        } else {
                            color = 1 << 24 | (r as usize) << 16 | (g as usize) << 8 | b as usize;
                        }
                    } else {
                        println!("erresc(38/48): Incorrect number of parameters: expected three\n");
                    }
                }
                5 => {
                    /* indexed color */
                    if let Some(c) = getcolor(params) {
                        if c <= 255 {
                            color = c as usize;
                        } else {
                            println!(
                                "erresc(38/48): Incorrect parameter {} greater than 255\n",
                                c
                            );
                        }
                    } else {
                        println!("erresc(38/48): Incorrect parameter (too few or invalid)\n");
                    }
                }
                0 => {} /* implemented defined (only foreground) */
                1 => {} /* transparent */
                3 => {} /* direct color in CMY space */
                4 => {} /* direct color in CMYK space */
                x => {
                    println!("erresc(38/48): gfx attr {} unknown\n", x);
                }
            }
        } else {
            println!("unknown color glyph attr");
        }
        color
    }

    fn set_glyph_attr(&mut self, params: &Params) {
        let prop = &mut self.term.c.glyph.prop;

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
                    9 =>
                    /* X10 mouse compatibility mode */
                    {
                        self.win.set_pointer_motion(false);
                        self.win.set_mode(WinMode::MOUSE, false);
                        self.win.set_mode(WinMode::MOUSEX10, val);
                    }
                    1000 =>
                    /* 1000: report button press */
                    {
                        self.win.set_pointer_motion(false);
                        self.win.set_mode(WinMode::MOUSE, false);
                        self.win.set_mode(WinMode::MOUSEBTN, val);
                    }
                    1002 =>
                    /* 1002: report motion on button press */
                    {
                        self.win.set_pointer_motion(false);
                        self.win.set_mode(WinMode::MOUSE, false);
                        self.win.set_mode(WinMode::MOUSEMOTION, val);
                    }
                    1003 =>
                    /* 1003: enable all mouse motions */
                    {
                        self.win.set_pointer_motion(val);
                        self.win.set_mode(WinMode::MOUSE, false);
                        self.win.set_mode(WinMode::MOUSEMANY, val);
                    }
                    1004 =>
                    /* 1004: send focus events to tty */
                    {
                        self.win.set_mode(WinMode::FOCUS, val);
                    }
                    1006 =>
                    /* 1006: extended reporting mode */
                    {
                        self.win.set_mode(WinMode::MOUSESGR, val);
                    }
                    1034 => self.win.set_mode(WinMode::EIGHT_BIT, val),
                    47 | 1047 => {
                        /* swap screen */
                        let alt = self.term.in_mode(TermMode::ALTSCREEN);
                        if alt {
                            self.term.clear_region(0..self.term.cols, 0..self.term.rows);
                        }
                        if val ^ alt {
                            self.term.swap_screen()
                        }
                    }
                    1048 => self.term.save_load_cursor(val),
                    1049 => {
                        /* swap screen & set/restore cursor as xterm */
                        self.term.save_load_cursor(val);
                        let alt = self.term.in_mode(TermMode::ALTSCREEN);
                        if alt {
                            self.term.clear_region(0..self.term.cols, 0..self.term.rows);
                        }
                        if val ^ alt {
                            self.term.swap_screen()
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

    fn send_color_osc(&mut self, idx: usize, leader: &str, bell_terminated: bool) {
        let mut v: Vec<u8> = vec![0x1b];
        v.extend_from_slice(
            format!("]{};{}", leader, self.win.get_color_osc(idx).unwrap()).as_bytes(),
        );
        if bell_terminated {
            v.push(0x07);
        } else {
            v.push(0x1b);
            v.push(b'\\');
        }
        self.term.pty.write(v);
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
                win.reset_colors();
                // reset title...
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
                /* color set, color index;spec */
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
                                    println!("OSC 4 error: {}", err.msg);
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
                            println!("OSC 10 error: {}", err.msg);
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
                            println!("OSC 11 error: {}", err.msg);
                        }
                    } else {
                        println!("OSC 11, to many parameters");
                    }
                } else {
                    println!("OSC 11, missing color name");
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
                                println!("OSC 104 error: {}", err.msg);
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
                        println!("OSC 110 error: {}", err.msg);
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
                        println!("OSC 111 error: {}", err.msg);
                    }
                } else {
                    println!("OSC 111 takes no parameters");
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
