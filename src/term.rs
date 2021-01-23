// FIXME: wide char
// FIXME: merge control with esc

use std::cmp;
use std::mem;
use std::iter;
use std::os::raw::*;
use std::fs::File;
use std::ops::RangeBounds;
use std::convert::TryFrom;

use crate::{
    utils::{
        is_set,
        is_control,
        is_between,
        limit,
        mod_flag,
        sort_pair,
        assert_range,
    },
    esc::{
        EscBuf,
        Esc,
        Csi,
    },
    sys,
    utf8,
    Result,
};

const COLS_MIN:  usize = 1;
const COLS_MAX:  usize = u16::MAX as usize;
const ROWS_MIN:  usize = 1;
const ROWS_MAX:  usize = u16::MAX as usize;

const VTIDEN: &[u8] = b"\x1B[?6c";

// glyph font attribute
pub const ATTR_NULL:       u32 = 0;
pub const ATTR_BOLD:       u32 = 1 << 0;
pub const ATTR_FAINT:      u32 = 1 << 1;
pub const ATTR_ITALIC:     u32 = 1 << 2;
pub const ATTR_UNDERLINE:  u32 = 1 << 3;
pub const ATTR_BLINK:      u32 = 1 << 4;
pub const ATTR_REVERSE:    u32 = 1 << 5;
pub const ATTR_INVISIBLE:  u32 = 1 << 6;
pub const ATTR_STRUCK:     u32 = 1 << 7;
pub const ATTR_FONT_MASK:  u32 = (1 << 8) - 1;
pub const ATTR_BOLD_FAINT: u32 = ATTR_BOLD | ATTR_FAINT;

// glyph width attribute
const ATTR_WRAP:       u32 = 1 << 8;
const ATTR_WIDE:       u32 = 1 << 9;
const ATTR_WDUMMY:     u32 = 1 << 10;

#[derive(Clone, Copy)]
pub struct Glyph {
    pub u:    u32,    // character code
    pub attr: u32,    // attribute flags
    pub fg:   u8,     // foreground index
    pub bg:   u8,     // background index
}

impl Glyph {
    pub fn new(u: u32, attr: u32, fg: u8, bg: u8) -> Self {
        Glyph { u, attr, fg, bg }
        
    }
}

// cursor state flags
//
// WRAPNEXT is when cursor at the right of the terminal, it does
// automatic wrap to new line until new character input.
//
// ORIGIN is for cursor movement relative to scroll region.
const CURSOR_DEFAULT:  u32 = 0;
const CURSOR_WRAPNEXT: u32 = 1;
const CURSOR_ORIGIN:   u32 = 2;

#[derive(Clone)]
pub struct TCursor {
    pub g: Glyph,     // current char attributes
    pub x: usize,
    pub y: usize,
    pub state: u32,
}

impl TCursor {
    pub fn new(g: Glyph) -> Self {
        TCursor { g: g, x: 0, y: 0, state: CURSOR_DEFAULT }
    }
}

// term mode flags
const MODE_WRAP:      u32 = 1 << 0;
const MODE_INSERT:    u32 = 1 << 1;
const MODE_ALTSCREEN: u32 = 1 << 2;
const MODE_CRLF:      u32 = 1 << 3;
const MODE_ECHO:      u32 = 1 << 4;
const MODE_PRINT:     u32 = 1 << 5;
const MODE_UTF8:      u32 = 1 << 6;
const MODE_DEFAULT:   u32 = MODE_UTF8 | MODE_WRAP;

pub struct Term {
    mode: u32,

    rows: usize,
    cols: usize,
    top:  usize,               // top    scroll limit
    bot:  usize,               // bottom scroll limit

    lines: Vec<Vec<Glyph>>,
    dirty: Vec<bool>,
    tabs:  Vec<bool>,

    fg:     u8,
    bg:     u8,
    ocx:    usize,
    ocy:    usize,
    c_save: TCursor,
    c:      TCursor,

    ptyfd: c_int,               // pty fd
    pid:   c_int,               // child pid

    blank: Glyph,
    unread: Vec<u8>,
    esc: EscBuf,
}

impl Term {
    pub fn new(cols: usize, rows: usize, fg: u8, bg: u8) -> Result<Self> {
        let blank = Glyph::new(' ' as u32, 0, fg, bg);
        let mut c: TCursor = TCursor::new(blank);

        let (pid, ptyfd) = sys::forkpty()?;
        if pid == 0 {
            sys::execsh();
        }

        let mut term = Term {
            mode: MODE_DEFAULT,

            rows: 0,
            cols: 0,
            top:  0,
            bot:  0,

            lines: Vec::new(),
            dirty: Vec::new(),
            tabs:  Vec::new(),

            fg:     c.g.fg,
            bg:     c.g.bg,
            ocx:    c.x,
            ocy:    c.y,
            c_save: c.clone(),
            c:      c,

            ptyfd: ptyfd,
            pid:   pid,

            blank: blank,
            unread: Vec::new(),
            esc: EscBuf::new(),
        };
        term.tresize(cols, rows);
        sys::setlocale()?;

        Ok(term)
    }

    pub fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    pub fn sync_last_pos(&mut self) {
        self.ocx = self.c.x;
        self.ocy = self.c.y;
    }

    pub fn get_last_pos(&self) -> (usize, usize) {
        (self.ocx, self.ocy)
    }

    pub fn get_cursor(&self) -> &TCursor {
        &self.c
    }

    pub fn get_ptyfd(&self) -> c_int {
        self.ptyfd
    }

    pub fn get_lines(&self) -> &Vec<Vec<Glyph>> {
        &self.lines
    }

    pub fn get_dirty(&self, y: usize) -> bool {
        self.dirty[y]
    }

    pub fn set_dirty(&mut self, y: usize, dirty: bool) {
        self.dirty[y] = dirty
    }

    pub fn tresize(&mut self, cols: usize, rows: usize) -> bool {
        let cols = limit(cols, COLS_MIN, COLS_MAX);
        let rows = limit(rows, ROWS_MIN, ROWS_MAX);

        if cols == self.cols && rows == self.rows {
            return false;
        }

        if self.c.y > rows - 1 {
            self.lines.rotate_left(self.c.y-rows+1)
        }
        self.lines.resize_with(rows, Vec::new);
        self.lines.shrink_to_fit();
        for line in self.lines.iter_mut() {
            line.resize(cols, self.blank);
            line.shrink_to_fit();
        }

        self.dirty.resize(rows, true);
        self.dirty.shrink_to_fit();
        for i in 0..cmp::min(self.rows, rows) {
            self.dirty[i] = true;
        }

        let mut i = self.cols;
        self.tabs.resize_with(cols, || {
            let t = i % 8 == 0;
            i += 1;
            t
        });
        self.tabs.shrink_to_fit();

        self.cols = cols;
        self.rows = rows;
        self.top = 0;
        self.bot = rows - 1;

        self.ocx = cmp::min(self.ocx, self.cols-1);
        self.ocy = cmp::min(self.ocy, self.rows-1);
        self.tmoveto(self.c.x, self.c.y);  // moveto apply limit

        sys::resizepty(self.ptyfd, self.cols, self.rows);
        true
    }

    pub fn ttywrite(&mut self, s: &[u8], may_echo: bool) -> Result<()> {
        if may_echo && is_set(self.mode, MODE_ECHO) {
            self.twrite(s, true);
        }

        if !is_set(self.mode, MODE_CRLF) {
            self.ttywriteraw(s)?;
            return Ok(());
        }

        let mut iter = s.split(|&c| c == b'\r');
        let first = iter.next().unwrap();
        self.ttywriteraw(first)?;
        for part in iter {
            self.ttywriteraw(b"\r\n")?;
            self.ttywriteraw(part)?;
        }

        Ok(())
    }

    pub fn ttyread(&mut self) -> Result<usize> {
        let mut buf = [0; 8192];

        let start = self.unread.len();
        if start > 0 {
            buf[..start].copy_from_slice(&self.unread);
        }

        let n = sys::read(self.ptyfd, &mut buf[start..])?;
        if n > 0 {
            let written = self.twrite(&buf[..start+n], false);
            self.unread = buf[written..start+n].to_owned();
        }

        Ok(n)
    }

    fn ttywriteraw(&mut self, s: &[u8]) -> Result<()> {
        let mut n = 0;

        while n < s.len() {
            let mut maxfd = 0;
            let mut rfdset = sys::fdset_new();
            let mut wfdset = sys::fdset_new();
            sys::fdset_set(&mut rfdset, self.ptyfd, &mut maxfd);
            sys::fdset_set(&mut wfdset, self.ptyfd, &mut maxfd);

            sys::select(
                maxfd+1, Some(&mut rfdset), Some(&mut wfdset), None, None
            ).unwrap();

            if sys::fdset_is_set(&mut rfdset, self.ptyfd) {
                self.ttyread()?;
            }
            if sys::fdset_is_set(&mut wfdset, self.ptyfd) {
                n += sys::write(self.ptyfd, &s[n..])?;
            }
        }

        Ok(())
    }

    fn twrite(&mut self, s: &[u8], show_ctrl: bool) -> usize {
        if !is_set(self.mode, MODE_UTF8) {
            for &c in s {
                self.tputbyte(c, show_ctrl);
            }
            return s.len();
        }

        let mut n = 0;

        while n < s.len() {
            let mut u = 0;
            let mut charsize = utf8::decode(&s[n..], &mut u);
            if charsize == 0 {
                break;
            }
            n += charsize;

            if u <= u8::MAX as u32 {
                self.tputbyte(u as u8, show_ctrl);
            } else {
                self.tputc(u);
            }
        }

        return n;
    }

    fn treset(&mut self) {
        for i in 0..self.cols {
            self.tabs[i] = i % 8 == 0;
        }

        self.top = 0;
        self.bot = self.rows - 1;
        self.mode = MODE_DEFAULT;

        self.tmoveto(0, 0);
        self.tcursor(true);
	self.tclearregion(0..self.cols, 0..self.rows);
    }

    fn resettitle(&mut self) {
        // FIXME
    }

    fn tnewline(&mut self, first_col: bool) {
        if self.c.y == self.bot {
            self.tscrollup(self.top, 1);
        } else {
            self.c.y += 1;
        }
        if first_col {
            self.c.x = 0;
        }
    }

    fn tsetscroll(&mut self, top: usize, bot: usize) {
	let mut top = cmp::min(top, self.rows-1);
	let mut bot = cmp::min(bot, self.rows-1);
        let (top, bot) = sort_pair(top, bot);

	self.top = top;
	self.bot = bot;
    }

    fn tscrollup(&mut self, orig: usize, n: usize) {
        assert!(is_between(orig, self.top, self.bot));
        if n < 1 {
            return;
        }
        let n = cmp::min(n, self.bot-orig+1);

        self.tclearregion(0..self.cols, orig..orig+n);
        self.tsetdirt(orig+n..=self.bot);
        self.lines[orig..=self.bot].rotate_left(n);
    }

    fn tscrolldown(&mut self, orig: usize, n: usize) {
        assert!(is_between(orig, self.top, self.bot));
        if n < 1 {
            return;
        }
	let n = cmp::min(n, self.bot-orig+1);

	self.tsetdirt(orig..self.bot-n+1);
	self.tclearregion(0..self.cols, self.bot-n+1..=self.bot);
        self.lines[orig..=self.bot].rotate_right(n);
    }

    fn tsetdirt<R: RangeBounds<usize>>(&mut self, range: R) {
        for i in assert_range(&range) {
            self.dirty[i] = true;
        }
    }

    fn tclearregion<R1, R2>(&mut self, xrange: R1, yrange: R2)
    where R1: RangeBounds<usize>, R2: RangeBounds<usize>,
    {
        for y in assert_range(&yrange) {
            self.dirty[y] = true;
            for x in assert_range(&xrange) {
                self.lines[y][x] = self.blank;
            }
        }
    }

    fn tmoveto(&mut self, x: usize, y: usize) {
        self.c.x = cmp::min(x, self.cols-1);
        if is_set(self.c.state, CURSOR_ORIGIN) {
            self.c.y = limit(y, self.top, self.bot);
        } else {
            self.c.y = cmp::min(y, self.rows-1);
        }
        mod_flag(&mut self.c.state, false, CURSOR_WRAPNEXT);
    }

    fn tmoveato(&mut self, x: usize, y: usize) {
        if is_set(self.c.state, CURSOR_ORIGIN) {
            self.tmoveto(x, y+self.top);
        } else {
            self.tmoveto(x, y);
        }
    }

    fn tputtab(&mut self, n: i32) {
        if n == 0 {
            return;
        }

        let mut n = n;
        if n > 0 {
            while n != 0 && self.c.x != self.cols - 1 {
                self.c.x += 1;
                if self.tabs[self.c.x] {
                    n -= 1;
                }
            }
        } else {
            while n != 0 && self.c.x != 0 {
                self.c.x -= 1;
                if self.tabs[self.c.x] {
                    n -= 1;
                }
            }
        }
    }

    fn tinsertblank(&mut self, n: usize) {
        let n = cmp::min(n, self.cols-self.c.x);

        self.lines[self.c.y].copy_within(self.c.x..self.cols-n, self.c.x+n);
        self.tclearregion(self.c.x..self.c.x+n, self.c.y..=self.c.y);
    }

    fn tdeletechar(&mut self, n: usize) {
        let n = cmp::min(n, self.cols-self.c.x);

        self.lines[self.c.y].copy_within(self.c.x+n..self.cols, self.c.x);
        self.tclearregion(self.cols-n..self.cols, self.c.y..=self.c.y);
    }

    fn tinsertblankline(&mut self, n: usize) {
        if is_between(self.c.y, self.top, self.bot) {
            self.tscrolldown(self.c.y, n);
        }
    }

    fn tdeleteline(&mut self, n: usize) {
        if is_between(self.c.y, self.top, self.bot) {
            self.tscrollup(self.c.y, n);
        }
    }

    fn tputbyte(&mut self, u: u8, show_ctrl: bool) {
        let mut u = u;

        if is_control(u) && show_ctrl {
            if u & 0x80 != 0 {
                u &= 0x7F;
                self.tputc('^' as u32);
                self.tputc('[' as u32);
            } else if ! b"\n\r\t".contains(&u) {
                u ^= 0x40;
                self.tputc('^' as u32);
            }
        }

        if is_control(u) {
            self.tcontrolcode(u);
            return;
        }

        if let Some(seq) = self.esc.input(u) {
            match seq {
                Esc::Eaten => (),
                Esc::Esc(x) => self.eschandle(x),
                Esc::Csi(x) => self.csihandle(x),
            }
            return;
        }

        self.tputc(u as u32);
    }

    fn tputc(&mut self, u: u32) {
        if is_set(self.c.state, CURSOR_WRAPNEXT) {
            self.tnewline(true);
            mod_flag(&mut self.c.state, false, CURSOR_WRAPNEXT);
        }

        self.dirty[self.c.y] = true;
        self.lines[self.c.y][self.c.x] = self.c.g;
        self.lines[self.c.y][self.c.x].u = u;

        self.c.x += 1;
        if self.c.x == self.cols {
            if is_set(self.mode, MODE_WRAP) {
                self.c.x = self.cols - 1;
                mod_flag(&mut self.c.state, true, CURSOR_WRAPNEXT);
            } else {
                self.c.x = 0;
            }
        }
    }

    fn tcursor(&mut self, save: bool) {
        if save {
            self.c_save = self.c.clone();
        } else {
            self.c = self.c_save.clone();
            self.tmoveto(self.c.x, self.c.y);  // may saved from a larger size
        }
    }

    fn tsetattr(&mut self, attrs: Vec<usize>) {
        let g = &mut self.c.g;

        for attr in attrs {
            match attr {
                0 => {
                    g.attr &= !ATTR_FONT_MASK;
                    g.fg = self.fg;
                    g.bg = self.bg;
                },
                1 => g.attr |= ATTR_BOLD,
                2 => g.attr |= ATTR_FAINT,
                3 => g.attr |= ATTR_ITALIC,
                4 => g.attr |= ATTR_UNDERLINE,
                5 | 6 => g.attr |= ATTR_BLINK,
                7 => g.attr |= ATTR_REVERSE,
                8 => g.attr |= ATTR_INVISIBLE,
                9 => g.attr |= ATTR_STRUCK,
                22 => g.attr &= !ATTR_BOLD_FAINT,
                23 => g.attr &= !ATTR_ITALIC,
                24 => g.attr &= !ATTR_UNDERLINE,
                25 => g.attr &= !ATTR_BLINK,
                27 => g.attr &= !ATTR_REVERSE,
                28 => g.attr &= !ATTR_INVISIBLE,
                29 => g.attr &= !ATTR_STRUCK,
                30..=37 => g.fg = (attr - 30) as u8,
                38 => (), // FIXME
                39 => g.fg = self.fg,
                40..=47 => g.bg = (attr - 40) as u8,
                48 => (), // FIXME
                49 => g.bg = self.bg,
                90..=97 => g.fg = (attr - 90 + 8) as u8,
                100..=107 => g.bg = (attr - 100 + 8) as u8,
                _ => println!("tsetattr unknown attr {}", attr),
            }
        }
    }

    fn tsetmode(&mut self, private: bool, set: bool, args: Vec<usize>) {
        for arg in &args {
            match (private, arg) {
                (true, 6) =>  /* DECOM -- Origin */
                {
                    mod_flag(&mut self.c.state, set, CURSOR_ORIGIN);
                    self.tmoveato(0, 0);
                },
		(true, 7) =>  /* DECAWM -- Auto wrap */
                    mod_flag(&mut self.mode, set, MODE_WRAP),
		(true, 1048) =>
                    self.tcursor(set),
		(true, 0)  |  /* Error (IGNORED) */
		(true, 2)  |  /* DECANM -- ANSI/VT52 (IGNORED) */
		(true, 3)  |  /* DECCOLM -- Column  (IGNORED) */
		(true, 4)  |  /* DECSCLM -- Scroll (IGNORED) */
		(true, 8)  |  /* DECARM -- Auto repeat (IGNORED) */
		(true, 18) |  /* DECPFF -- Printer feed (IGNORED) */
		(true, 19) |  /* DECPEX -- Printer extent (IGNORED) */
		(true, 42) |  /* DECNRCM -- National characters (IGNORED) */
		(true, 12) => /* att610 -- Start blinking cursor (IGNORED) */
                    (),
                (false, 0) =>  /* Error (IGNORED) */
                    (),
                (false, 4) => /* IRM -- Insertion-replacement */
                    mod_flag(&mut self.mode, set, MODE_INSERT),
                (false, 12) => /* SRM -- Send/Receive */
                    mod_flag(&mut self.mode, set, MODE_ECHO),
                (false, 20) => /* LNM -- Linefeed/new line */
                    mod_flag(&mut self.mode, set, MODE_CRLF),
                _ => println!("unknown tsetmode ({}, {:?})", private, args),
            }
        }
    }

    fn tcontrolcode(&mut self, u: u8) {
        match u {
            0x09 =>    /* HT */
                self.tputtab(1),
            0x08 =>    /* BS */
                self.tmoveto(self.c.x.saturating_sub(1), self.c.y),
            0x0D =>    /* CR */
                self.tmoveto(0, self.c.y),
            0x0C | 0x0B | 0x0A =>   /* LF VT LF */
                self.tnewline(is_set(self.mode, MODE_CRLF)),
            0x07 =>    /* BEL */
                (),
            0x1B =>    /* ESC */
                self.esc.start(),
            0x18 | 0x1A =>    /* CAN SUB */
                self.esc.end(),
            _ => (),
        }
    }

    fn eschandle(&mut self, u: u8) {
        match u {
	    b'D' => /* IND -- Linefeed */
            {
		if self.c.y == self.bot {
		    self.tscrollup(self.top, 1);
		} else {
		    self.tmoveto(self.c.x, self.c.y+1);
		}
            },
	    b'E' => /* NEL -- Next line */
	        self.tnewline(true), /* always go to first col */
	    b'H' => /* HTS -- Horizontal tab stop */
		self.tabs[self.c.x] = true,
	    b'M' => /* RI -- Reverse index */
            {
		if self.c.y == self.top {
		    self.tscrolldown(self.top, 1);
		} else {
		    self.tmoveto(self.c.x, self.c.y-1);
		}
            },
	    b'Z' => /* DECID -- Identify Terminal */
            {
                let _ = self.ttywrite(VTIDEN, false);
            },
	    b'c' => /* RIS -- Reset to initial state */
            {
	        self.treset();
		self.resettitle();
            },
	    b'7' => /* DECSC -- Save Cursor */
		self.tcursor(true),
	    b'8' => /* DECRC -- Restore Cursor */
		self.tcursor(false),
            b'=' |
            b'>' => (),
            _ => println!("unknown esc {}", u as char),
        }
    }

    fn csihandle(&mut self, csi: Csi) {
        let dft_arg0 = cmp::max(csi.args[0], 1);

        match csi.mode {
	    b'@' => /* ICH -- Insert <n> blank char */
		self.tinsertblank(dft_arg0),
	    b'A' => /* CUU -- Cursor <n> Up */
		self.tmoveto(self.c.x, self.c.y.saturating_sub(dft_arg0)),
	    b'B' |  /* CUD -- Cursor <n> Down */
	    b'e' => /* VPR -- Cursor <n> Down */
	        self.tmoveto(self.c.x, self.c.y+dft_arg0),
            b'c' if csi.args[0] == 0 => /* DA -- Device Attributes */
            {
                let _ = self.ttywrite(VTIDEN, false);
            },
            b'C' |  /* CUF -- Cursor <n> Forward */
            b'a' => /* HPR -- Cursor <n> Forward */
                self.tmoveto(self.c.x+dft_arg0, self.c.y),
	    b'D' => /* CUB -- Cursor <n> Backward */
		self.tmoveto(self.c.x.saturating_sub(dft_arg0), self.c.y),
	    b'E' => /* CNL -- Cursor <n> Down and first col */
		self.tmoveto(0, self.c.y+dft_arg0),
	    b'F' => /* CPL -- Cursor <n> Up and first col */
		self.tmoveto(0, self.c.y.saturating_sub(dft_arg0)),
	    b'g' => /* TBC -- Tabulation clear */
            {
                match csi.args[0] {
                    0 => /* clear current tab stop */
                        self.tabs[self.c.x] = false,
                    3 => /* clear all the tabs */
                    {
                        for i in 0..self.cols {
                            self.tabs[i] = false;
                        }
                    },
                    _ => println!("unknown {}", csi),
                }
            },
	    b'G'|   /* CHA -- Move to <col> */
	    b'`' => /* HPA */
		self.tmoveto(dft_arg0-1, self.c.y),
	    b'H' |  /* CUP -- Move to <row> <col> */
	    b'f' => /* HVP */
            {
                let mut arg1 = 1;
                if csi.args.len() >= 2 {
                    arg1 = cmp::max(csi.args[1], 1);
                }
		self.tmoveato(arg1-1, dft_arg0-1);
            },
	    b'I' => /* CHT -- Cursor Forward Tabulation <n> tab stops */
		self.tputtab(dft_arg0 as i32),
	    b'J' => /* ED -- Clear screen */
            {
                let y = self.c.y;
                match csi.args[0] {
                    0 => /* below */
                    {
			self.tclearregion(self.c.x..self.cols, y..=y);
			self.tclearregion(0..self.cols, y+1..self.rows);
                    },
                    1 => /* above */
                    {
			self.tclearregion(0..self.cols, 0..y);
			self.tclearregion(0..=self.c.x, y..=y);
                    },
		    2 => /* all */
			self.tclearregion(0..self.cols, 0..self.rows),
                    _ => println!("unknown {}", csi),
                }
            }
	    b'K' => /* EL erase line */
            {
                let y = self.c.y;
                match csi.args[0] {
                    0 => /* right */
                        self.tclearregion(self.c.x..self.cols, y..=y),
                    1 => /* left */
                        self.tclearregion(0..=self.c.x, y..=y),
                    2 => /* all */
                        self.tclearregion(0..self.cols, y..=y),
                    _ => println!("unknown {}", csi),
                }
            },
	    b'S' => /* SU -- Scroll <n> line up */
		self.tscrollup(self.top, dft_arg0),
	    b'T' => /* SD -- Scroll <n> line down */
		self.tscrolldown(self.top, dft_arg0),
	    b'L' => /* IL -- Insert <n> blank lines */
		self.tinsertblankline(dft_arg0),
	    b'l' => /* RM -- Reset Mode */
	        (),//self.tsetmode(csi.private, false, csi.args),
	    b'M' => /* DL -- Delete <n> lines */
		self.tdeleteline(dft_arg0),
	    b'X' => /* ECH -- Erase <n> char */
            {
	        self.tclearregion(
                    self.c.x..self.c.x+dft_arg0, self.c.y..=self.c.y
                );
            },
	    b'P' => /* DCH -- Delete <n> char */
		self.tdeletechar(dft_arg0),
	    b'Z' => /* CBT -- Cursor Backward Tabulation <n> tab stops */
		self.tputtab(-(dft_arg0 as i32)),
	    b'd' => /* VPA -- Move to <row> */
		self.tmoveato(self.c.x, dft_arg0-1),
	    b'h' => /* SM -- Set terminal mode */
		(),//self.tsetmode(csi.private, true, csi.args),
	    b'm' => /* SGR -- Terminal attribute (color) */
		self.tsetattr(csi.args),
	    b'n' => /* DSR – Device Status Report (cursor position) */
	    {
                if csi.args[0] == 6 {
                    let s = format!("\x1B[{};{}R", self.c.y+1, self.c.x+1);
                    let _ = self.ttywrite(s.as_bytes(), false);
                }
	    },
	    b'r' if !csi.private => /* DECSTBM -- Set Scrolling Region */
            {
                let mut arg1 = self.rows;
                if csi.args.len() >= 2 {
                    arg1 = cmp::min(csi.args[1], self.rows);
                }
		self.tsetscroll(dft_arg0-1, arg1-1);
		self.tmoveato(0, 0);
	    },
	    b's' => /* DECSC -- Save cursor position (ANSI.SYS) */
		self.tcursor(true),
	    b'u' => /* DECRC -- Restore cursor position (ANSI.SYS) */
		self.tcursor(false),
            _ => println!("unknown {}", csi),
        }
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        if let Err(e) = sys::send_sighup(self.pid) {
            eprintln!("{}", e.msg);
        }
    }
}
