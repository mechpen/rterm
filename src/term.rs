// FIXME: support wide chars

use bitflags::bitflags;
use unicode_width::UnicodeWidthChar;
use std::cmp;
use crate::utils::{
    is_between,
    limit,
    sort_pair,
};
use crate::glyph::{
    Glyph,
    GlyphAttr,
    blank_glyph,
};
use crate::snap::{
    SnapMode,
    is_delim,
};
use crate::cursor::Cursor;
use crate::point::Point;
use crate::pty::Pty;
use crate::Result;

struct Selection {
    pub mode:  SnapMode,
    pub empty: bool,
    pub ob:    Point,
    pub oe:    Point,
    pub nb:    Point,
    pub ne:    Point,
}

impl Selection {
    pub fn new() -> Self {
        Selection {
            mode: SnapMode::None, empty: true,
            ob: Point::new(0, 0), oe: Point::new(0, 0),
            nb: Point::new(0, 0), ne: Point::new(0, 0),
        }
    }
}

bitflags! {
    pub struct Mode: u32 {
        const WRAP        = 1 << 0;
        const INSERT      = 1 << 1;
        const ORIGIN      = 1 << 2;
        const CRLF        = 1 << 3;
    }
}

pub const COLS_MIN: usize = 2;
pub const COLS_MAX: usize = u16::MAX as usize;
pub const ROWS_MIN: usize = 1;
pub const ROWS_MAX: usize = u16::MAX as usize;
const TAB_STOP: usize = 8;

pub struct Term {
    pub rows: usize,
    pub cols: usize,
    pub c: Cursor,
    pub lines: Vec<Vec<Glyph>>,
    pub dirty: Vec<bool>,
    pub pty: Pty,
    pub scroll_top: usize,
    pub scroll_bot: usize,
    tabs: Vec<bool>,
    mode: Mode,
    sel: Selection,
}

impl Term {
    pub fn new(cols: usize, rows: usize) -> Result<Self> {
        let mut term = Term {
            rows: 0,
            cols: 0,
            c: Cursor::new(),
            lines: Vec::new(),
            dirty: Vec::new(),
            pty: Pty::new()?,
            scroll_top: 0,
            scroll_bot: 0,
            mode: Mode::empty(),
            tabs: Vec::new(),
            sel: Selection::new(),
        };

        term.resize(cols, rows);
        Ok(term)
    }

    pub fn resize(&mut self, cols: usize, rows: usize) -> bool {
        let cols = limit(cols, COLS_MIN, COLS_MAX);
        let rows = limit(rows, ROWS_MIN, ROWS_MAX);

        if cols == self.cols && rows == self.rows {
            return false;
        }

        if self.c.y > rows - 1 {
            self.scroll_up(0, self.c.y-rows+1)
        }

        self.lines.resize_with(rows, Vec::new);
        self.lines.shrink_to_fit();
        for line in self.lines.iter_mut() {
            line.resize(cols, blank_glyph());
            line.shrink_to_fit();
        }

        self.dirty.resize(rows, true);
        self.dirty.shrink_to_fit();
        for i in 0..cmp::min(self.rows, rows) {
            self.dirty[i] = true;
        }

        let mut i = self.cols;
        self.tabs.resize_with(cols, || {
            let t = i % TAB_STOP == 0;
            i += 1;
            t
        });
        self.tabs.shrink_to_fit();

        self.scroll_top = 0;
        self.scroll_bot = rows - 1;
        self.cols = cols;
        self.rows = rows;
        self.pty.resize(cols, rows).unwrap();

        // relocate cursor
        self.move_to(self.c.x, self.c.y);
        true
    }

    pub fn reset(&mut self) {
        self.clear_lines(0..self.rows);
        for i in 0..self.cols {
            self.tabs[i] = i % TAB_STOP == 0;
        }

        self.c.reset();
        self.scroll_top = 0;
        self.scroll_bot = self.rows - 1;
    }

    pub fn set_scroll(&mut self, top: usize, bot: usize) {
        let top = cmp::min(top, self.rows-1);
        let bot = cmp::min(bot, self.rows-1);
        let (top, bot) = sort_pair(top, bot);

        self.scroll_top = top;
        self.scroll_bot = bot;
    }

    fn set_dirty<R: Iterator<Item = usize>>(&mut self, range: R) {
        for i in range {
            self.dirty[i] = true;
        }
    }

    pub fn clear_region<R1, R2>(&mut self, xrange: R1, yrange: R2)
    where R1: Iterator<Item = usize> + Clone, R2: Iterator<Item = usize>,
    {
        for y in yrange {
            self.dirty[y] = true;
            for x in xrange.clone() {
                self.lines[y][x] = blank_glyph();
                if self.selected(x, y) {
                    self.selection_clear();
                }
            }
        }
    }

    fn clear_lines<R: Iterator<Item = usize>>(&mut self, range: R) {
        self.clear_region(0..self.cols, range)
    }

    pub fn new_line(&mut self) {
        if self.c.y == self.scroll_bot {
            self.scroll_up(self.scroll_top, 1);
        } else {
            self.c.y += 1;
        }
    }

    pub fn scroll_up(&mut self, orig: usize, n: usize) {
        assert!(is_between(orig, self.scroll_top, self.scroll_bot));
        if n < 1 {
            return;
        }
        let n = cmp::min(n, self.scroll_bot-orig+1);

        self.clear_lines(orig..orig+n);
        self.set_dirty(orig+n..=self.scroll_bot);
        self.lines[orig..=self.scroll_bot].rotate_left(n);

        self.selection_scroll(orig, -(n as i32));
    }

    pub fn scroll_down(&mut self, orig: usize, n: usize) {
        assert!(is_between(orig, self.scroll_top, self.scroll_bot));
        if n < 1 {
            return;
        }
        let n = cmp::min(n, self.scroll_bot-orig+1);

        self.set_dirty(orig..self.scroll_bot-n+1);
        self.clear_lines(self.scroll_bot-n+1..=self.scroll_bot);
        self.lines[orig..=self.scroll_bot].rotate_right(n);

        self.selection_scroll(orig, n as i32);
    }

    pub fn insert_lines(&mut self, n: usize) {
        if is_between(self.c.y, self.scroll_top, self.scroll_bot) {
            self.scroll_down(self.c.y, n);
        }
    }

    pub fn delete_lines(&mut self, n: usize) {
        if is_between(self.c.y, self.scroll_top, self.scroll_bot) {
            self.scroll_up(self.c.y, n);
        }
    }

    pub fn move_to(&mut self, x: usize, y: usize) {
        self.c.x = cmp::min(x, self.cols-1);

        if self.mode.contains(Mode::ORIGIN) {
            self.c.y = limit(y, self.scroll_top, self.scroll_bot);
        } else {
            self.c.y = cmp::min(y, self.rows-1);
        }

        self.c.wrap_next = false;
    }

    pub fn insert_blanks(&mut self, n: usize) {
        let n = cmp::min(n, self.cols-self.c.x);

        self.lines[self.c.y].copy_within(self.c.x..self.cols-n, self.c.x+n);
        self.clear_region(self.c.x..self.c.x+n, self.c.y..=self.c.y);
    }

    pub fn delete_chars(&mut self, n: usize) {
        let n = cmp::min(n, self.cols-self.c.x);

        self.lines[self.c.y].copy_within(self.c.x+n..self.cols, self.c.x);
        self.clear_region(self.cols-n..self.cols, self.c.y..=self.c.y);
    }

    pub fn put_tabs(&mut self, n: i32) {
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

    pub fn put_char(&mut self, c: char) {
        let width = UnicodeWidthChar::width(c).unwrap_or(0);
        if width == 0 {
            return
        }

        if self.c.wrap_next {
            // for wide chars, c is not at cols-1
            self.lines[self.c.y][self.cols-1].prop.attr.insert(GlyphAttr::WRAP);
            self.new_line();
            self.c.x = 0;
            self.c.wrap_next = false;
        }

        if self.mode.contains(Mode::INSERT) && self.c.x + width < self.cols {
            self.insert_blanks(width);
        }

	if self.c.x + width > self.cols {
	    self.new_line();
            self.c.x = 0;
	}

        if self.selected(self.c.x, self.c.y) {
            self.selection_clear();
        }

        self.dirty[self.c.y] = true;
        self.lines[self.c.y][self.c.x].prop = self.c.glyph.prop;
        self.lines[self.c.y][self.c.x].c = c;
        for x in self.c.x+1..width {
            self.lines[self.c.y][x].prop.attr.set(GlyphAttr::DUMMY, true);
        }

        self.c.x += width;
        if self.c.x == self.cols {
            if self.mode.contains(Mode::WRAP) {
                self.c.x -= width;
                self.c.wrap_next = true;
            } else {
                self.c.x = 0;
            }
        }
    }

    pub fn put_string(&mut self, string: String) {
        string.chars().for_each(|c| self.put_char(c));
    }

    pub fn save_cursor(&mut self) {
        self.c.save_pos();
    }

    pub fn load_cursor(&mut self) {
        self.c.load_pos();
        self.move_to(self.c.x, self.c.y);
    }

    pub fn clear_tabs<R: Iterator<Item = usize>>(&mut self, range: R) {
        for i in range {
            self.tabs[i] = false;
        }
    }

    pub fn selection_start(&mut self, x: usize, y: usize, mode: SnapMode) {
        // clear previous selection
        self.selection_clear();
        self.set_dirty(self.sel.nb.y ..= self.sel.ne.y);

        self.sel.mode = mode;
        self.sel.ob.x = cmp::min(x, self.cols-1);
        self.sel.ob.y = cmp::min(y, self.rows-1);
        self.sel.oe.x = self.sel.ob.x;
        self.sel.oe.y = self.sel.ob.y;

        match self.sel.mode {
            SnapMode::None => (),
            _ => self.selection_normalize(),
        }
    }

    pub fn selection_extend(&mut self, x: usize, y: usize) {
        self.sel.oe.x = cmp::min(x, self.cols-1);
        self.sel.oe.y = cmp::min(y, self.rows-1);
        self.selection_normalize();
    }

    pub fn selected(&self, x: usize, y: usize) -> bool {
        !self.sel.empty &&
            is_between(y, self.sel.nb.y, self.sel.ne.y) &&
            (y != self.sel.nb.y || x >= self.sel.nb.x) &&
            (y != self.sel.ne.y || x <= self.sel.ne.x)
    }

    pub fn selection_clear(&mut self) {
        self.sel.empty = true;
    }

    pub fn selection_get_content(&self) -> Option<String> {
        if self.sel.empty {
            return None
        }

        let mut string = String::new();

        for y in self.sel.nb.y ..= self.sel.ne.y {
            let start = if y == self.sel.nb.y {
                self.sel.nb.x
            } else {
                0
            };

            let end = if y == self.sel.ne.y {
                self.sel.ne.x
            } else {
                self.cols - 1
            };

            let text_end = cmp::min(end+1, self.text_len(y));
            for x in start .. text_end {
                string.push(self.lines[y][x].c);
            }

            if end == self.cols - 1 && !self.is_wrap_line(y) {
                string.push('\n');
            }
        }

        Some(string)
    }

    fn selection_normalize(&mut self) {
        let (mut nb, mut ne) = sort_pair(self.sel.ob, self.sel.oe);

        match self.sel.mode {
            SnapMode::None => {
                let end = self.text_len(nb.y);
                if nb.x > end {
                    nb.x = end;
                }
                if self.text_len(ne.y) <= ne.x {
                    ne.x = self.cols - 1;
                }
            },
            SnapMode::Word => {
                nb = self.snap_word(nb, Self::prev);
                ne = self.snap_word(ne, Self::next);
            },
            SnapMode::Line => {
                nb.x = 0;
                while nb.y > 0 && self.is_wrap_line(nb.y-1) {
                    nb.y -= 1;
                }
                ne.x = self.cols - 1;
                while ne.y < self.rows - 1 && self.is_wrap_line(ne.y) {
                    ne.y += 1;
                }
            },
        }

        let bot = cmp::min(nb.y, self.sel.nb.y);
        let top = cmp::max(ne.y, self.sel.ne.y);
        self.set_dirty(bot ..= top);

        self.sel.empty = false;
        self.sel.nb = nb;
        self.sel.ne = ne;
    }

    fn selection_scroll(&mut self, orig: usize, n: i32) {
        if self.sel.empty {
            return;
        }

        if !is_between(self.sel.ob.y, orig, self.scroll_bot) ||
            !is_between(self.sel.ob.y, orig, self.scroll_bot) {
            self.selection_clear();
            return;
        }

        let by = self.sel.ob.y as i32 + n;
        let ey = self.sel.oe.y as i32 + n;
        if !is_between(by, orig as i32, self.scroll_bot as i32) ||
            !is_between(ey, orig as i32, self.scroll_bot as i32) {
            self.selection_clear();
            return;
        }

        self.sel.ob.y = by as usize;
        self.sel.oe.y = ey as usize;
        self.selection_normalize();
    }

    fn prev(&self, p: &Point) -> Option<Point> {
        if p.x > 0 {
            return Some(Point::new(p.x-1, p.y))
        }
        if p.y > 0 {
            let p = Point::new(self.cols-1, p.y-1);
            if self.lines[p.y][p.x].prop.attr.contains(GlyphAttr::WRAP) {
                return Some(p)
            }
        }
        None
    }

    fn next(&self, p: &Point) -> Option<Point> {
        if p.x < self.cols - 1 {
            return Some(Point::new(p.x+1, p.y))
        }
        if p.y < self.rows - 1 {
            if self.lines[p.y][p.x].prop.attr.contains(GlyphAttr::WRAP) {
                return Some(Point::new(0, p.y+1))
            }
        }
        None
    }

    fn snap_word<F>(&self, point: Point, f: F) -> Point
    where F: Fn(&Self, &Point)->Option<Point>
    {
        let c = self.lines[point.y][point.x].c;
        let delim = is_delim(c);

        let mut point = point;
        while let Some(next_p) = f(self, &point) {
            let next_c = self.lines[next_p.y][next_p.x].c;
            if next_c != c && (delim || is_delim(next_c)) {
                break;
            }
            point = next_p;
        }
        point
    }

    fn text_len(&self, y: usize) -> usize {
        let mut x = self.cols;
        if self.is_wrap_line(y) {
            return x
        }
        while x > 0 && self.lines[y][x-1].c == ' ' {
            x -= 1
        }
        x
    }

    fn is_wrap_line(&self, y: usize) -> bool {
        self.lines[y][self.cols-1].prop.attr.contains(GlyphAttr::WRAP)
    }
}
