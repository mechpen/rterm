// FIXME: support wide chars

use crate::charset::CharsetTable;
use crate::color::{BG_COLOR, CURSOR_COLOR, CURSOR_REV_COLOR, FG_COLOR};
use crate::cursor::Cursor;
use crate::glyph::{blank_glyph, Glyph, GlyphAttr, GlyphProp};
use crate::point::Point;
use crate::pty::Pty;
use crate::snap::{is_delim, SnapMode};
use crate::utils::{is_between, limit, sort_pair};
use crate::Result;
use bitflags::bitflags;
use std::cmp;
use unicode_width::UnicodeWidthChar;

struct Selection {
    pub mode: SnapMode,
    pub empty: bool,
    pub ob: Point,
    pub oe: Point,
    pub nb: Point,
    pub ne: Point,
}

impl Selection {
    pub fn new() -> Self {
        Selection {
            mode: SnapMode::None,
            empty: true,
            ob: Point::new(0, 0),
            oe: Point::new(0, 0),
            nb: Point::new(0, 0),
            ne: Point::new(0, 0),
        }
    }
}

bitflags! {
    pub struct TermMode: u32 {
        const WRAP        = 1 << 0;
        const INSERT      = 1 << 1;
        const ORIGIN      = 1 << 2;
        const CRLF        = 1 << 3;
        const ALTSCREEN   = 1 << 4;
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
    pub dirty: Vec<bool>,
    pub pty: Pty,
    pub scroll_top: usize,
    pub scroll_bot: usize,
    pub charset: CharsetTable,
    pub prop: GlyphProp,
    saved_c: Option<Cursor>,
    saved_alt_c: Option<Cursor>,
    // lines contains the main and alternate screens, access it through the
    // lines() and lines_mut() functions to not have to worry about indexing.
    lines: Vec<Vec<Glyph>>,
    tabs: Vec<bool>,
    mode: TermMode,
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
            charset: CharsetTable::new(),
            prop: GlyphProp::new(FG_COLOR, BG_COLOR, GlyphAttr::empty()),
            mode: TermMode::WRAP,
            tabs: Vec::new(),
            sel: Selection::new(),
            saved_c: None,
            saved_alt_c: None,
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
            self.scroll_up(0, self.c.y - rows + 1)
        }

        // Double size to hold the main and alternate screens.
        self.lines.resize_with(rows * 2, Vec::new);
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

    pub fn set_mode(&mut self, mode: TermMode, val: bool) {
        self.mode.set(mode, val);
    }

    pub fn in_mode(&self, mode: TermMode) -> bool {
        self.mode.contains(mode)
    }

    pub fn get_glyph(&self, x: usize, y: usize) -> Glyph {
        let mut g = self.lines()[y][x];
        g.prop = g.prop.resolve(self.is_selected(x, y));
        g
    }

    pub fn get_glyph_at_cursor(&self) -> Glyph {
        let (x, y) = (self.c.x, self.c.y);
        let mut g = self.lines()[y][x];
        if self.is_selected(x, y) {
            g.prop.bg = CURSOR_REV_COLOR;
        } else {
            g.prop.bg = CURSOR_COLOR;
        }
        g
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
        let top = cmp::min(top, self.rows - 1);
        let bot = cmp::min(bot, self.rows - 1);
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
    where
        R1: Iterator<Item = usize> + Clone,
        R2: Iterator<Item = usize>,
    {
        let mut glyph = blank_glyph();
        glyph.prop = self.prop;
        for y in yrange {
            self.dirty[y] = true;
            for x in xrange.clone() {
                self.lines_mut()[y][x].clear(glyph);
                if self.is_selected(x, y) {
                    self.clear_selection();
                }
            }
        }
    }

    fn clear_lines<R: Iterator<Item = usize>>(&mut self, range: R) {
        self.clear_region(0..self.cols, range)
    }

    pub fn new_line(&mut self, first_col: bool) {
        if self.c.y == self.scroll_bot {
            self.scroll_up(self.scroll_top, 1);
        } else {
            self.c.y += 1;
        }

        if first_col {
            self.c.x = 0;
        }
    }

    pub fn scroll_up(&mut self, orig: usize, n: usize) {
        assert!(is_between(orig, self.scroll_top, self.scroll_bot));
        if n < 1 {
            return;
        }
        let bottom = self.scroll_bot;
        let n = cmp::min(n, bottom - orig + 1);

        self.clear_lines(orig..orig + n);
        self.set_dirty(orig + n..=bottom);
        self.lines_mut()[orig..=bottom].rotate_left(n);

        self.scroll_selection(orig, -(n as i32));
    }

    pub fn scroll_down(&mut self, orig: usize, n: usize) {
        assert!(is_between(orig, self.scroll_top, self.scroll_bot));
        if n < 1 {
            return;
        }
        let bottom = self.scroll_bot;
        let n = cmp::min(n, bottom - orig + 1);

        self.set_dirty(orig..bottom - n + 1);
        self.clear_lines(bottom - n + 1..=self.scroll_bot);
        self.lines_mut()[orig..=bottom].rotate_right(n);

        self.scroll_selection(orig, n as i32);
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

    // move to a y pos that is not derived from a previous y pos
    pub fn move_ato(&mut self, x: usize, y: usize) {
        let mut y = y;
        if self.mode.contains(TermMode::ORIGIN) {
            y += self.scroll_top;
        }
        self.move_to(x, y);
    }

    pub fn move_to(&mut self, x: usize, y: usize) {
        self.c.x = cmp::min(x, self.cols - 1);
        if self.mode.contains(TermMode::ORIGIN) {
            self.c.y = limit(y, self.scroll_top, self.scroll_bot);
        } else {
            self.c.y = cmp::min(y, self.rows - 1);
        }
        self.c.wrap_next = false;
    }

    pub fn insert_blanks(&mut self, n: usize) {
        let (x, y) = (self.c.x, self.c.y);
        let n = cmp::min(n, self.cols - x);

        let source = x..self.cols - n;
        let dest = x + n;
        self.lines_mut()[y].copy_within(source, dest);
        self.clear_region(x..x + n, y..=y);
    }

    pub fn delete_chars(&mut self, n: usize) {
        let (x, y, cols) = (self.c.x, self.c.y, self.cols);
        let n = cmp::min(n, cols - x);

        self.lines_mut()[y].copy_within(x + n..cols, x);
        self.clear_region(cols - n..cols, y..=y);
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
            return;
        }

        let cols = self.cols;

        if self.c.wrap_next {
            let y = self.c.y;
            // for wide chars, cursor is not at cols-1
            self.lines_mut()[y][cols - 1]
                .prop
                .attr
                .insert(GlyphAttr::WRAP);
            self.new_line(true);
            self.c.wrap_next = false;
        }

        if self.mode.contains(TermMode::INSERT) && self.c.x + width < cols {
            self.insert_blanks(width);
        }

        if self.c.x + width > cols {
            self.new_line(true);
        }

        if self.is_selected(self.c.x, self.c.y) {
            self.clear_selection();
        }

        // x, y may have updated.
        let (x, y) = (self.c.x, self.c.y);
        self.dirty[y] = true;
        self.lines_mut()[y][x].prop = self.prop;
        self.lines_mut()[y][x].c = c;
        for x2 in x + 1..x + width {
            self.lines_mut()[y][x2].prop.attr.insert(GlyphAttr::DUMMY);
        }

        self.c.x += width;
        if self.c.x == cols {
            if self.mode.contains(TermMode::WRAP) {
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
        if self.mode.contains(TermMode::ALTSCREEN) {
            self.saved_alt_c = Some(self.c);
        } else {
            self.saved_c = Some(self.c);
        }
    }

    pub fn load_cursor(&mut self) {
        let saved = if self.mode.contains(TermMode::ALTSCREEN) {
            self.saved_alt_c
        } else {
            self.saved_c
        };
        if let Some(saved) = saved {
            self.c = saved;
            self.move_to(self.c.x, self.c.y);
        }
    }

    pub fn save_load_cursor(&mut self, save: bool) {
        if save {
            self.save_cursor();
        } else {
            self.load_cursor();
        }
    }

    pub fn clear_tabs<R: Iterator<Item = usize>>(&mut self, range: R) {
        for i in range {
            self.tabs[i] = false;
        }
    }

    pub fn set_tab(&mut self, x: usize) {
        self.tabs[x] = true;
    }

    pub fn start_selection(&mut self, x: usize, y: usize, mode: SnapMode) {
        // clear previous selection
        self.clear_selection();
        self.set_dirty(self.sel.nb.y..=self.sel.ne.y);

        self.sel.mode = mode;
        self.sel.ob.x = cmp::min(x, self.cols - 1);
        self.sel.ob.y = cmp::min(y, self.rows - 1);
        self.sel.oe.x = self.sel.ob.x;
        self.sel.oe.y = self.sel.ob.y;

        match self.sel.mode {
            SnapMode::None => (),
            _ => self.normalize_selection(),
        }
    }

    pub fn extend_selection(&mut self, x: usize, y: usize) {
        self.sel.oe.x = cmp::min(x, self.cols - 1);
        self.sel.oe.y = cmp::min(y, self.rows - 1);
        self.normalize_selection();
    }

    pub fn is_selected(&self, x: usize, y: usize) -> bool {
        !self.sel.empty
            && is_between(y, self.sel.nb.y, self.sel.ne.y)
            && (y != self.sel.nb.y || x >= self.sel.nb.x)
            && (y != self.sel.ne.y || x <= self.sel.ne.x)
    }

    pub fn clear_selection(&mut self) {
        self.sel.empty = true;
    }

    pub fn get_selection_content(&self) -> Option<String> {
        if self.sel.empty {
            return None;
        }

        let mut string = String::new();

        for y in self.sel.nb.y..=self.sel.ne.y {
            let start = if y == self.sel.nb.y { self.sel.nb.x } else { 0 };

            let end = if y == self.sel.ne.y {
                self.sel.ne.x
            } else {
                self.cols - 1
            };

            let text_end = cmp::min(end + 1, self.text_len(y));
            for x in start..text_end {
                string.push(self.lines()[y][x].c);
            }

            if end == self.cols - 1 && !self.is_wrap_line(y) {
                string.push('\n');
            }
        }

        Some(string)
    }

    pub fn swap_screen(&mut self) {
        self.mode ^= TermMode::ALTSCREEN;
        self.set_dirty(0..self.dirty.len());
    }

    pub fn setdirtattr(&mut self, attr: GlyphAttr) {
        let start = self.first_line();
        for i in 0..self.rows {
            for g in self.lines[start + i].iter() {
                if g.prop.attr.contains(attr) {
                    self.dirty[i] = true;
                    break;
                }
            }
        }
    }

    fn normalize_selection(&mut self) {
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
            }
            SnapMode::Word => {
                nb = self.snap_word(nb, Self::prev);
                ne = self.snap_word(ne, Self::next);
            }
            SnapMode::Line => {
                nb.x = 0;
                while nb.y > 0 && self.is_wrap_line(nb.y - 1) {
                    nb.y -= 1;
                }
                ne.x = self.cols - 1;
                while ne.y < self.rows - 1 && self.is_wrap_line(ne.y) {
                    ne.y += 1;
                }
            }
        }

        let bot = cmp::min(nb.y, self.sel.nb.y);
        let top = cmp::max(ne.y, self.sel.ne.y);
        self.set_dirty(bot..=top);

        self.sel.empty = false;
        self.sel.nb = nb;
        self.sel.ne = ne;
    }

    fn scroll_selection(&mut self, orig: usize, n: i32) {
        if self.sel.empty {
            return;
        }

        if !is_between(self.sel.ob.y, orig, self.scroll_bot)
            || !is_between(self.sel.ob.y, orig, self.scroll_bot)
        {
            self.clear_selection();
            return;
        }

        let by = self.sel.ob.y as i32 + n;
        let ey = self.sel.oe.y as i32 + n;
        if !is_between(by, orig as i32, self.scroll_bot as i32)
            || !is_between(ey, orig as i32, self.scroll_bot as i32)
        {
            self.clear_selection();
            return;
        }

        self.sel.ob.y = by as usize;
        self.sel.oe.y = ey as usize;
        self.normalize_selection();
    }

    fn prev(&self, p: &Point) -> Option<Point> {
        if p.x > 0 {
            return Some(Point::new(p.x - 1, p.y));
        }
        if p.y > 0 {
            let p = Point::new(self.cols - 1, p.y - 1);
            if self.lines()[p.y][p.x].prop.attr.contains(GlyphAttr::WRAP) {
                return Some(p);
            }
        }
        None
    }

    fn next(&self, p: &Point) -> Option<Point> {
        if p.x < self.cols - 1 {
            return Some(Point::new(p.x + 1, p.y));
        }
        if p.y < self.rows - 1 && self.lines()[p.y][p.x].prop.attr.contains(GlyphAttr::WRAP) {
            return Some(Point::new(0, p.y + 1));
        }
        None
    }

    fn snap_word<F>(&self, point: Point, f: F) -> Point
    where
        F: Fn(&Self, &Point) -> Option<Point>,
    {
        let c = self.lines()[point.y][point.x].c;
        let delim = is_delim(c);

        let mut point = point;
        while let Some(next_p) = f(self, &point) {
            let next_c = self.lines()[next_p.y][next_p.x].c;
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
            return x;
        }
        while x > 0 && self.lines()[y][x - 1].c == ' ' {
            x -= 1
        }
        x
    }

    fn is_wrap_line(&self, y: usize) -> bool {
        self.lines()[y][self.cols - 1]
            .prop
            .attr
            .contains(GlyphAttr::WRAP)
    }

    #[inline]
    fn first_line(&self) -> usize {
        if self.mode.contains(TermMode::ALTSCREEN) {
            self.rows
        } else {
            0
        }
    }

    #[inline]
    fn lines(&self) -> &[Vec<Glyph>] {
        if self.mode.contains(TermMode::ALTSCREEN) {
            &self.lines[self.rows..]
        } else {
            &self.lines[0..self.rows]
        }
    }

    #[inline]
    fn lines_mut(&mut self) -> &mut [Vec<Glyph>] {
        if self.mode.contains(TermMode::ALTSCREEN) {
            &mut self.lines[self.rows..]
        } else {
            &mut self.lines[0..self.rows]
        }
    }
}
