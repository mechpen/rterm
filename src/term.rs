// FIXME: support wide chars

use crate::charset::CharsetTable;
use crate::color::{BG_COLOR, CURSOR_COLOR, CURSOR_REV_COLOR, FG_COLOR};
use crate::cursor::Cursor;
use crate::glyph::{blank_glyph, Glyph, GlyphAttr, GlyphProp};
use crate::point::Point;
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

/// Term row is basically an iterator but can not implement Iterator without
/// a lot of allocations so we have this.
pub struct TermRow<'row> {
    row: &'row [Glyph],
    pos: usize,
    x: usize,
}

impl<'row> TermRow<'row> {
    pub fn new(row: &[Glyph]) -> TermRow {
        TermRow { row, pos: 0, x: 0 }
    }

    /// Get next glyph and properties from a row.
    /// glyph will contain the chars making up the glyph (grapheme cluster- usually
    /// just one but maybe more).  Returns the current row number (0 based) and the
    /// glyph properties for the glyph or None if the row has been traversed.
    pub fn next(&mut self, glyph: &mut Vec<char>) -> Option<(usize, GlyphProp)> {
        let len = self.row.len();
        if self.pos >= len {
            return None;
        }
        let x = self.x;
        self.x += 1;
        glyph.clear();
        let prop = self.row[self.pos].prop;
        glyph.push(self.row[self.pos].c);
        self.pos += 1;
        while self.pos < len && self.row[self.pos].prop.attr.contains(GlyphAttr::CLUSTER) {
            glyph.push(self.row[self.pos].c);
            self.pos += 1;
        }
        Some((x, prop))
    }

    /// Return the glyph and properties for column x of the row.  Returns None of
    /// the column is too large or if the TermRow has already advanced past x.
    /// This advances the TermRow.
    pub fn column(&mut self, x: usize, glyph: &mut Vec<char>) -> Option<GlyphProp> {
        while let Some((cx, prop)) = self.next(glyph) {
            if x == cx {
                return Some(prop);
            }
        }
        None
    }
}

pub struct Term {
    pub rows: usize,
    pub cols: usize,
    pub c: Cursor,
    pub dirty: Vec<bool>,
    pub scroll_top: usize,
    pub scroll_bot: usize,
    pub charset: CharsetTable,
    pub prop: GlyphProp,
    saved_c: Option<Cursor>,
    saved_alt_c: Option<Cursor>,
    // lines contains the main and alternate screens, access it through the
    // lines() and lines_mut() functions to not have to worry about indexing.
    // Each row of lines may be larger then cols to support grapheme clusters.
    // The common case will be one codepoint per glyph so this structure should
    // still be fine but when accessing lines have to consider this.
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

        if !self.sel.empty {
            self.clear_selection();
        }

        if self.c.y > rows - 1 {
            self.scroll_up(0, self.c.y - rows + 1)
        }

        // Double size to hold the main and alternate screens.
        self.lines.resize_with(rows * 2, Vec::new);
        self.lines.shrink_to_fit();
        for line in self.lines.iter_mut() {
            // Account for any grapheme clusters in line.
            let x = Self::adjust_x_line(line, cols);
            line.resize(x, blank_glyph());
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

    pub fn get_row(&self, row: usize) -> TermRow {
        TermRow::new(&self.lines()[row])
    }

    pub fn get_glyph(&self, x: usize, y: usize, glyph: &mut Vec<char>) -> GlyphProp {
        if let Some(prop) = self.get_row(y).column(x, glyph) {
            prop.resolve(self.is_selected(x, y))
        } else {
            GlyphProp::new(0, 0, GlyphAttr::empty())
        }
    }

    pub fn get_glyph_at_cursor(&self, glyph: &mut Vec<char>) -> GlyphProp {
        let (x, y) = (self.c.x, self.c.y);
        let mut prop = self.get_glyph(x, y, glyph);
        if self.is_selected(x, y) {
            prop.bg = CURSOR_REV_COLOR;
        } else {
            prop.bg = CURSOR_COLOR;
        }
        prop
    }

    pub fn reset(&mut self) {
        self.clear_lines(0..self.rows);
        for i in 0..self.cols {
            self.tabs[i] = i % TAB_STOP == 0;
        }

        self.c.reset();
        self.prop.reset();
        self.mode = TermMode::WRAP;
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
            let mut shrink = 0;
            let len = self.lines()[y].len();
            let mut startx = None;
            let mut endx = 0;
            let mut offset = 0;
            for x in xrange.clone() {
                if startx.is_none() {
                    let ax = self.adjust_x(y, x);
                    startx = Some(ax);
                    offset = ax - x;
                }
                let x = x + offset;
                if (x + shrink) >= len {
                    break;
                }
                while self.lines()[y][x + shrink]
                    .prop
                    .attr
                    .contains(GlyphAttr::CLUSTER)
                {
                    self.lines_mut()[y][x + shrink].clear(glyph);
                    shrink += 1;
                    if (x + shrink) >= len {
                        break;
                    }
                }
                if (x + shrink) >= len {
                    break;
                }
                self.lines_mut()[y][x + shrink].clear(glyph);
                if self.is_selected(x, y) {
                    self.clear_selection();
                }
                endx = x;
            }
            if endx + 1 + shrink < len {
                endx += 1;
                while self.lines()[y][endx + shrink]
                    .prop
                    .attr
                    .contains(GlyphAttr::CLUSTER)
                {
                    self.lines_mut()[y][endx + shrink].clear(glyph);
                    shrink += 1;
                    if (endx + shrink) >= len {
                        break;
                    }
                }
            }
            if shrink > 0 {
                if let Some(startx) = startx {
                    self.lines_mut()[y].copy_within(startx + shrink.., startx);
                    self.lines_mut()[y].resize(len - shrink, glyph);
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

        let ax = self.adjust_x(y, x);
        let source = ax..self.lines()[y].len() - n;
        let dest = ax + n;
        self.lines_mut()[y].copy_within(source, dest);
        self.clear_region(x..x + n, y..=y);
        let cols = self.cols;
        let acols = self.adjust_x(y, cols);
        self.lines_mut()[y].resize(acols, blank_glyph());
    }

    pub fn delete_chars(&mut self, n: usize) {
        let (x, y, cols) = (self.c.x, self.c.y, self.cols);
        let n = cmp::min(n, cols - x);

        let ax = self.adjust_x(y, x);
        let nx = self.adjust_x(y, x + n);
        let acols = self.lines()[y].len();
        self.lines_mut()[y].copy_within(nx..acols, ax);
        // Could have malformed garbage if grapheme clusters were at the end
        // so just resize down then back to the proper size vs clear_region().
        self.lines_mut()[y].resize(ax + (acols - nx), blank_glyph());
        let acols = self.adjust_x(y, cols);
        self.lines_mut()[y].resize(acols, blank_glyph());
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

    fn adjust_x_line(line: &[Glyph], mut x: usize) -> usize {
        let mut i = 0;
        while i <= x {
            if let Some(g) = line.get(i) {
                if g.prop.attr.contains(GlyphAttr::CLUSTER) {
                    x += 1;
                }
            }
            i += 1;
        }
        x
    }

    fn adjust_x(&self, row: usize, x: usize) -> usize {
        Self::adjust_x_line(&self.lines()[row], x)
    }

    pub fn put_char(&mut self, c: char) {
        let width = if let Some(w) = UnicodeWidthChar::width(c) {
            w
        } else {
            // Indicates a control code.
            return;
        };

        let cols = self.cols;

        if width > 0 && self.c.wrap_next {
            let y = self.c.y;
            let x = self.adjust_x(y, cols - 1);
            // for wide chars, cursor is not at cols-1
            self.lines_mut()[y][x].prop.attr.insert(GlyphAttr::WRAP);
            self.new_line(true);
            self.c.wrap_next = false;
        }
        if width == 0 {
            let y = self.c.y;
            let x = self.adjust_x(self.c.y, self.c.x);
            self.lines_mut()[y].insert(x, blank_glyph());
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
        // skip past any extra codepoints from grapheme clusters.
        let x = self.adjust_x(y, x);
        self.dirty[y] = true;
        {
            let prop = self.prop;
            let lines = self.lines_mut();
            if lines[y].len() <= x {
                lines[y].push(blank_glyph());
            }
            lines[y][x].prop = prop;
            lines[y][x].c = c;
            let mut shrink = 0;
            let mut i = x + 1;
            while let Some(g) = lines[y].get(i) {
                if !g.prop.attr.contains(GlyphAttr::CLUSTER) {
                    break;
                }
                i += 1;
                shrink += 1;
            }
            if shrink > 0 {
                let startx = x + 1;
                lines[y].copy_within(startx + shrink.., startx);
                lines[y].resize(lines[y].len() - shrink, blank_glyph());
            }
            if width == 0 {
                lines[y][x].prop.attr.insert(GlyphAttr::CLUSTER);
            } else {
                for x2 in x + 1..x + width {
                    lines[y][x2].prop.attr.insert(GlyphAttr::DUMMY);
                }
            }
        }

        if width > 0 {
            self.c.x += width;
            if self.c.x == cols {
                if self.mode.contains(TermMode::WRAP) {
                    //self.c.x -= width;
                    self.c.wrap_next = true;
                } else {
                    self.c.x = 0;
                }
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
        if !self.sel.empty {
            self.clear_selection();
        }

        self.sel.ob.x = cmp::min(x, self.cols - 1);
        self.sel.ob.y = cmp::min(y, self.rows - 1);
        self.sel.oe.x = self.sel.ob.x;
        self.sel.oe.y = self.sel.ob.y;

        self.sel.mode = mode;
        self.sel.empty = self.sel.mode == SnapMode::None;
        self.normalize_selection();
    }

    pub fn extend_selection(&mut self, x: usize, y: usize) {
        self.clear_selection();

        self.sel.oe.x = cmp::min(x, self.cols - 1);
        self.sel.oe.y = cmp::min(y, self.rows - 1);

        self.sel.empty = false;
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
        self.set_dirty(self.sel.nb.y..=self.sel.ne.y);
    }

    pub fn get_selection_content(&self) -> Option<String> {
        if self.sel.empty {
            return None;
        }

        let mut string = String::new();
        let mut glyph = Vec::new();

        for y in self.sel.nb.y..=self.sel.ne.y {
            let start = if y == self.sel.nb.y { self.sel.nb.x } else { 0 };

            let end = if y == self.sel.ne.y {
                self.sel.ne.x
            } else {
                self.cols - 1
            };

            let text_end = cmp::min(end + 1, self.text_len(y));
            let mut row = self.get_row(y);
            while let Some((x, _)) = row.next(&mut glyph) {
                if x >= start && x < text_end {
                    for c in &glyph {
                        string.push(*c);
                    }
                }
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

        self.set_dirty(nb.y..=ne.y);
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
        // XXX - fix x
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
        let mut ax = self.lines()[y].len();
        while x > 0 && self.lines()[y][ax - 1].c == ' ' {
            ax -= 1;
            x -= 1
        }
        x
    }

    fn is_wrap_line(&self, y: usize) -> bool {
        let len = self.lines()[y].len();
        self.lines()[y][len - 1].prop.attr.contains(GlyphAttr::WRAP)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resize() -> Result<()> {
        let mut term = Term::new(80, 25)?;
        assert_eq!(term.lines.len(), 50); // both primary and alternate screen.
        for l in &term.lines {
            assert_eq!(l.len(), 80);
        }
        term.resize(100, 30);
        assert_eq!(term.lines.len(), 60); // both primary and alternate screen.
        for l in &term.lines {
            assert_eq!(l.len(), 100);
        }
        term.resize(50, 10);
        assert_eq!(term.lines.len(), 20); // both primary and alternate screen.
        for l in &term.lines {
            assert_eq!(l.len(), 50);
        }

        term.resize(80, 25);
        assert_eq!(term.lines.len(), 50);
        term.move_to(0, 2);
        for _i in 0..80 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        assert_eq!(term.lines[1].len(), 80);
        assert_eq!(term.lines[2].len(), 240);
        assert_eq!(term.lines[3].len(), 80);
        term.resize(40, 25);
        assert_eq!(term.lines[1].len(), 40);
        assert_eq!(term.lines[2].len(), 120);
        assert_eq!(term.lines[3].len(), 40);
        let mut row = term.get_row(1);
        let mut max_x = 0;
        let mut glyph = Vec::new();
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 1);
            assert_eq!(glyph[0], ' ');
            max_x = x;
        }
        assert_eq!(max_x, 39);
        let mut row = term.get_row(2);
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 3);
            assert_eq!(glyph[0], 'e');
            assert_eq!(glyph[1] as u32, 0x0300);
            assert_eq!(glyph[2] as u32, 0x0302);
            max_x = x;
        }
        assert_eq!(max_x, 39);
        Ok(())
    }

    #[test]
    fn test_clusters() -> Result<()> {
        let mut term = Term::new(80, 25)?;
        let mut max_x = 0;
        term.move_to(10, 5);
        term.put_string("e\u{0300}\u{0302}".to_string());
        assert_eq!(term.lines.len(), 50);
        assert_eq!(term.lines[4].len(), 80);
        assert_eq!(term.lines[5].len(), 82);
        assert_eq!(term.lines[6].len(), 80);
        let mut row = term.get_row(5);
        let mut glyph = Vec::new();
        while let Some((x, _prop)) = row.next(&mut glyph) {
            if x == 10 {
                assert_eq!(glyph.len(), 3);
                assert_eq!(glyph[0], 'e');
                assert_eq!(glyph[1] as u32, 0x0300);
                assert_eq!(glyph[2] as u32, 0x0302);
            } else {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);
        term.move_to(0, 3);
        for _i in 0..80 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        assert_eq!(term.lines.len(), 50);
        assert_eq!(term.lines[2].len(), 80);
        assert_eq!(term.lines[3].len(), 240);
        assert_eq!(term.lines[4].len(), 80);
        assert_eq!(term.lines[5].len(), 82);
        assert_eq!(term.lines[6].len(), 80);
        let mut row = term.get_row(3);
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 3);
            assert_eq!(glyph[0], 'e');
            assert_eq!(glyph[1] as u32, 0x0300);
            assert_eq!(glyph[2] as u32, 0x0302);
            max_x = x;
        }
        assert_eq!(max_x, 79);

        term.move_to(0, 5);
        for _i in 0..160 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        assert_eq!(term.lines.len(), 50);
        assert_eq!(term.lines[2].len(), 80);
        assert_eq!(term.lines[3].len(), 240);
        assert_eq!(term.lines[4].len(), 80);
        assert_eq!(term.lines[6].len(), 240);
        assert_eq!(term.lines[7].len(), 80);
        let mut row = term.get_row(3);
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 3);
            assert_eq!(glyph[0], 'e');
            assert_eq!(glyph[1] as u32, 0x0300);
            assert_eq!(glyph[2] as u32, 0x0302);
            max_x = x;
        }
        assert_eq!(max_x, 79);
        let mut row = term.get_row(4);
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 1);
            assert_eq!(glyph[0], ' ');
            max_x = x;
        }
        assert_eq!(max_x, 79);
        let mut row = term.get_row(5);
        while let Some((x, prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 3);
            assert_eq!(glyph[0], 'e');
            assert_eq!(glyph[1] as u32, 0x0300);
            assert_eq!(glyph[2] as u32, 0x0302);
            if x == 79 {
                assert_eq!(true, prop.attr.contains(GlyphAttr::WRAP));
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);
        let mut row = term.get_row(6);
        while let Some((x, prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 3);
            assert_eq!(glyph[0], 'e');
            assert_eq!(glyph[1] as u32, 0x0300);
            assert_eq!(glyph[2] as u32, 0x0302);
            if x == 79 {
                assert_eq!(false, prop.attr.contains(GlyphAttr::WRAP));
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);
        let mut row = term.get_row(7);
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 1);
            assert_eq!(glyph[0], ' ');
            max_x = x;
        }
        assert_eq!(max_x, 79);
        term.put_string("e\u{0300}\u{0302}".to_string());
        let mut row = term.get_row(6);
        while let Some((x, prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 3);
            assert_eq!(glyph[0], 'e');
            assert_eq!(glyph[1] as u32, 0x0300);
            assert_eq!(glyph[2] as u32, 0x0302);
            if x == 79 {
                assert_eq!(true, prop.attr.contains(GlyphAttr::WRAP));
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);
        let mut row = term.get_row(7);
        while let Some((x, _prop)) = row.next(&mut glyph) {
            if x == 0 {
                assert_eq!(glyph.len(), 3);
                assert_eq!(glyph[0], 'e');
                assert_eq!(glyph[1] as u32, 0x0300);
                assert_eq!(glyph[2] as u32, 0x0302);
            } else {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);

        // Go ahead an check reset.
        term.reset();
        assert_eq!(term.lines.len(), 50);
        for y in 0..25 {
            assert_eq!(term.lines[y].len(), 80);
            let mut row = term.get_row(y);
            max_x = 0;
            while let Some((x, _prop)) = row.next(&mut glyph) {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
                max_x = x;
            }
            assert_eq!(max_x, 79);
        }
        Ok(())
    }

    #[test]
    fn test_delete_chars() -> Result<()> {
        let mut term = Term::new(80, 25)?;
        let mut max_x;
        let mut glyph = Vec::new();
        term.move_to(0, 0);
        for _i in 0..80 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        term.move_to(0, 0);
        term.delete_chars(80);
        term.move_to(10, 10);
        for _i in 0..8 {
            term.put_string("x".to_string());
        }

        term.move_to(10, 10);
        term.delete_chars(8);
        assert_eq!(term.lines.len(), 50);
        for y in 0..25 {
            assert_eq!(term.lines[y].len(), 80);
            let mut row = term.get_row(y);
            max_x = 0;
            while let Some((x, _prop)) = row.next(&mut glyph) {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
                max_x = x;
            }
            assert_eq!(max_x, 79);
        }

        term.move_to(10, 2);
        for _i in 0..10 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        for _i in 0..10 {
            term.put_char('x');
        }
        term.move_to(10, 2);
        term.delete_chars(10);
        let mut row = term.get_row(2);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 1);
            if x > 9 && x < 20 {
                assert_eq!(glyph[0], 'x');
            } else {
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);

        term.reset();
        term.move_to(0, 5);
        for i in 0..80 {
            if i % 2 == 1 {
                term.put_string("e\u{0300}\u{0302}".to_string());
            } else {
                term.put_char('x');
            }
        }
        for i in 0..80 {
            let i = 79 - i;
            if i % 2 == 1 {
                term.move_to(i, 5);
                term.delete_chars(1);
            }
        }
        let mut row = term.get_row(5);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 1);
            if x < 40 {
                assert_eq!(glyph[0], 'x');
            } else {
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);

        term.reset();
        term.move_to(0, 5);
        for i in 0..80 {
            if i % 2 == 1 {
                term.put_string("e\u{0300}".to_string());
            } else {
                term.put_char('x');
            }
        }
        assert_eq!(term.lines()[5].len(), 120);
        for i in 0..80 {
            let i = 79 - i;
            if i % 2 == 1 {
                term.move_to(i, 5);
                term.delete_chars(1);
            }
        }
        assert_eq!(term.lines()[5].len(), 80);
        let mut row = term.get_row(5);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            assert_eq!(glyph.len(), 1);
            if x < 40 {
                assert_eq!(glyph[0], 'x');
            } else {
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);

        term.reset();
        term.move_to(0, 5);
        for i in 0..80 {
            if i % 2 == 1 {
                term.put_string("e\u{0300}".to_string());
            } else {
                term.put_char('x');
            }
        }
        assert_eq!(term.lines()[5].len(), 120);
        for i in 0..80 {
            let i = 79 - i;
            if i % 2 == 0 {
                term.move_to(i, 5);
                term.delete_chars(1);
            }
        }
        assert_eq!(term.lines()[5].len(), 120);
        let mut row = term.get_row(5);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            if x < 40 {
                assert_eq!(glyph.len(), 2);
                assert_eq!(glyph[0], 'e');
                assert_eq!(glyph[1], '\u{0300}');
            } else {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);
        Ok(())
    }

    #[test]
    fn test_insert_blanks() -> Result<()> {
        let mut term = Term::new(80, 25)?;
        let mut max_x;
        let mut glyph = Vec::new();
        term.move_to(0, 0);
        assert_eq!(term.lines()[0].len(), 80);
        term.insert_blanks(80);
        assert_eq!(term.lines()[0].len(), 80);
        term.move_to(0, 0);
        for _i in 0..80 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        assert_eq!(term.lines()[0].len(), 240);
        term.move_to(0, 0);
        term.insert_blanks(80);
        assert_eq!(term.lines()[0].len(), 80);
        for y in 0..25 {
            assert_eq!(term.lines[y].len(), 80);
            let mut row = term.get_row(y);
            max_x = 0;
            while let Some((x, _prop)) = row.next(&mut glyph) {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
                max_x = x;
            }
            assert_eq!(max_x, 79);
        }

        term.reset();
        term.move_to(0, 5);
        for _i in 0..80 {
            term.put_string("e\u{0302}".to_string());
        }
        assert_eq!(term.lines()[5].len(), 160);
        term.move_to(0, 5);
        term.insert_blanks(80);
        assert_eq!(term.lines()[5].len(), 80);
        for y in 0..25 {
            assert_eq!(term.lines[y].len(), 80);
            let mut row = term.get_row(y);
            max_x = 0;
            while let Some((x, _prop)) = row.next(&mut glyph) {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
                max_x = x;
            }
            assert_eq!(max_x, 79);
        }

        term.move_to(0, 0);
        for _i in 0..80 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        assert_eq!(term.lines()[0].len(), 240);
        term.move_to(0, 0);
        term.insert_blanks(40);
        assert_eq!(term.lines()[0].len(), 160);
        let mut row = term.get_row(0);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            if x < 40 {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
            } else {
                assert_eq!(glyph.len(), 3);
                assert_eq!(glyph[0], 'e');
                assert_eq!(glyph[1], '\u{0300}');
                assert_eq!(glyph[2], '\u{0302}');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);

        term.move_to(0, 0);
        for _i in 0..80 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        assert_eq!(term.lines()[0].len(), 240);
        term.move_to(40, 0);
        term.insert_blanks(40);
        assert_eq!(term.lines()[0].len(), 160);
        let mut row = term.get_row(0);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            if x < 40 {
                assert_eq!(glyph.len(), 3);
                assert_eq!(glyph[0], 'e');
                assert_eq!(glyph[1], '\u{0300}');
                assert_eq!(glyph[2], '\u{0302}');
            } else {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);

        term.move_to(0, 0);
        for _i in 0..80 {
            term.put_string("e\u{0300}".to_string());
        }
        assert_eq!(term.lines()[0].len(), 160);
        term.move_to(40, 0);
        term.insert_blanks(40);
        /*let mut row = term.get_row(0);
        while let Some((x, _prop)) = row.next(&mut glyph) {
            println!("XXXX {} {:?}", x, glyph)
        }*/
        assert_eq!(term.lines()[0].len(), 120);
        let mut row = term.get_row(0);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            if x < 40 {
                assert_eq!(glyph.len(), 2);
                assert_eq!(glyph[0], 'e');
                assert_eq!(glyph[1], '\u{0300}');
            } else {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);

        term.move_to(0, 0);
        for _i in 0..80 {
            term.put_string("e\u{0300}\u{0302}".to_string());
        }
        assert_eq!(term.lines()[0].len(), 240);
        term.move_to(10, 0);
        term.insert_blanks(10);
        assert_eq!(term.lines()[0].len(), 220);
        let mut row = term.get_row(0);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            if x < 10 || x > 19 {
                assert_eq!(glyph.len(), 3);
                assert_eq!(glyph[0], 'e');
                assert_eq!(glyph[1], '\u{0300}');
                assert_eq!(glyph[2], '\u{0302}');
            } else {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);

        term.move_to(0, 0);
        for _i in 0..80 {
            term.put_char('x');
        }
        assert_eq!(term.lines()[0].len(), 80);
        term.move_to(10, 0);
        term.insert_blanks(10);
        assert_eq!(term.lines()[0].len(), 80);
        let mut row = term.get_row(0);
        max_x = 0;
        while let Some((x, _prop)) = row.next(&mut glyph) {
            if x < 10 || x > 19 {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], 'x');
            } else {
                assert_eq!(glyph.len(), 1);
                assert_eq!(glyph[0], ' ');
            }
            max_x = x;
        }
        assert_eq!(max_x, 79);
        Ok(())
    }
}
