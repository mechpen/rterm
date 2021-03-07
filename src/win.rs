use bitflags::bitflags;
use std::slice;
use std::ptr::null_mut;
use std::os::raw::*;
use std::os::unix::io::RawFd;
use crate::color::{
    COLOR_NAMES,
    bg_color,
};
use crate::glyph::{
    GlyphProp,
    GlyphAttr,
};
use crate::utils::term_decode;
use crate::shortcut::find_shortcut;
use crate::keymap::map_key;
use crate::snap::Snap;
use crate::term::Term;
use crate::app::app_exit;
use crate::x11_wrapper as x11;
use crate::Result;

bitflags! {
    pub struct Mode: u32 {
        const APPKEYPAD   = 1 << 2;
        const MOUSEBTN    = 1 << 3;
        const MOUSEMOTION = 1 << 4;
        const REVERSE     = 1 << 5;
        const KBDLOCK     = 1 << 6;
        const HIDE        = 1 << 7;
        const APPCURSOR   = 1 << 8;
        const MOUSESGR    = 1 << 9;
        const EIGHTBIT    = 1 << 10;
        const BLINK       = 1 << 11;
        const FBLINK      = 1 << 12;
        const FOCUS       = 1 << 13;
        const MOUSEX10    = 1 << 14;
        const MOUSEMANY   = 1 << 15;
        const BRCKTPASTE  = 1 << 16;
        const NUMLOCK     = 1 << 17;
        const ECHO        = 1 << 18;
    }
}

pub struct Win {
    visible: bool,
    focused: bool,
    mode: Mode,

    dpy: x11::Display,
    win: x11::Window,
    scr: c_int,
    buf: x11::Pixmap,
    gc: x11::GC,
    colors: Vec<x11::XftColor>,
    draw: x11::XftDraw,
    font: x11::XftFont,
    cw: usize,
    ch: usize,
    ca: usize,

    sel_type: x11::Atom,
    sel_snap: Snap,
    sel_text: Option<String>,

    wm_protocols: x11::Atom,
    wm_delete_window: x11::Atom,
}

impl Win {
    pub fn new(cols: usize, rows: usize, font: Option<&str>) -> Result<Self> {
        let dpy = x11::XOpenDisplay()?;
        let scr = x11::XDefaultScreen(dpy);
        let vis = x11::XDefaultVisual(dpy, scr);
        let root = x11::XRootWindow(dpy, scr);

        let font = font.unwrap_or("monospace");
        let font = x11::XftFontOpenName(dpy, scr, font)?;
        let (cw, ch) = x11::font_size(font);
        let ca = x11::font_ascent(font);
        let (width, height) = (cols*cw, rows*ch);

        let cmap = x11::XDefaultColormap(dpy, scr);
        let mut colors = Vec::with_capacity(COLOR_NAMES.len());
        for &name in COLOR_NAMES {
            let color = x11::XftColorAllocName(dpy, vis, cmap, name);
            colors.push(color);
        }

        let depth = x11::XDefaultDepth(dpy, scr);
        let attributes_mask = x11::CW_BACK_PIXEL
            | x11::CW_COLOR_MAP
            | x11::CW_EVENT_MASK;
        let mut attributes: x11::XSetWindowAttributes = x11::zeroed();
        attributes.colormap = cmap;
        attributes.background_pixel = colors[bg_color()].pixel;
        attributes.event_mask = x11::KEY_PRESS_MASK
            | x11::VISIBILITY_CHANGE_MASK
            | x11::STRUCTURE_NOTIFY_MASK
            | x11::BUTTON_MOTION_MASK
            | x11::BUTTON_PRESS_MASK
            | x11::BUTTON_RELEASE_MASK;

        let win = x11::XCreateWindow(
            dpy, root, 0, 0, width, height, 0, depth,
            x11::INPUT_OUTPUT, vis, attributes_mask, &mut attributes,
        );
        x11::XStoreName(dpy, win, "rterm");

        let mut gcvalues: x11::XGCValues = x11::zeroed();
        gcvalues.graphics_exposures = x11::False;
        let gc = x11::XCreateGC(dpy, root, x11::GC_GRAPHICS_EXPOSURES, &mut gcvalues);

        let buf = x11::XCreatePixmap(dpy, win, width, height, depth);
        let draw = x11::XftDrawCreate(dpy, buf, vis, cmap);

        let cursor = x11::XCreateFontCursor(dpy, x11::XC_XTERM);
        x11::XDefineCursor(dpy, win, cursor);

        let wm_protocols = x11::XInternAtom(dpy, "WM_PROTOCOLS", x11::False);
        let wm_delete_window = x11::XInternAtom(dpy, "WM_DELETE_WINDOW", x11::False);
        let mut protocols = [wm_delete_window];
        x11::XSetWMProtocols(dpy, win, &mut protocols);

        let sel_type = x11::XInternAtom(dpy, "UTF8_STRING", x11::False);

        x11::XMapWindow(dpy, win);
        x11::XSync(dpy, x11::False);

        loop {
            let mut xev = x11::XNextEvent(dpy);
            if x11::XFilterEvent(&mut xev, win) == x11::True {
                continue;
            }
            if x11::event_type(&xev) == x11::MAP_NOTIFY {
                break;
            }
        }

        Ok(Win {
            visible: true,
            focused: true,
            mode: Mode::empty(),

            sel_type: sel_type,
            sel_snap: Snap::new(),
            sel_text: None,

            dpy,
            win,
            scr,

            gc,
            colors,
            buf,
            draw,
            font,
            cw,
            ch,
            ca,

            wm_protocols,
            wm_delete_window,
        })
    }

    pub fn fd(&self) -> RawFd {
        x11::XConnectionNumber(self.dpy)
    }

    pub fn bell(&self) {
    }

    pub fn undraw_cursor(&mut self, term: &Term) {
        let g = &term.lines[term.c.y][term.c.x];
        let prop = g.prop.resolve(term.selected(term.c.x, term.c.y));
        self.draw_cells(&[g.c], prop, term.c.x*self.cw, term.c.y*self.ch);
    }

    pub fn draw(&mut self, term: &mut Term) {
        if !self.visible {
            return;
        }

        for y in 0..term.rows {
            if !term.dirty[y] {
                continue;
            }
            self.draw_line(term, y);
            term.dirty[y] = false;
        }

        let c = term.lines[term.c.y][term.c.x].c;
        let reverse = !term.selected(term.c.x, term.c.y);
        let prop = term.c.glyph.prop.resolve(reverse);
        self.draw_cells(&[c], prop, term.c.x*self.cw, term.c.y*self.ch);

        self.finish_draw(term.cols, term.rows);
    }

    pub fn process_input(&mut self, term: &mut Term) {
        while x11::XPending(self.dpy) > 0 {
            let mut xev = x11::XNextEvent(self.dpy);
            if x11::XFilterEvent(&mut xev, self.win) == x11::True {
                continue;
            }

            let xev_type = x11::event_type(&xev);
            match xev_type {
                x11::KEY_PRESS =>
                    self.key_press(xev, term),
                x11::CLIENT_MESSAGE =>
                    self.client_message(xev),
                x11::CONFIGURE_NOTIFY =>
                    self.configure_notify(xev, term),
                x11::VISIBILITY_NOTIFY =>
                    self.visibility_notify(xev),
                x11::UNMAP_NOTIFY =>
                    self.unmap_notify(xev),
                x11::MOTION_NOTIFY =>
                    self.motion_notify(xev, term),
                x11::BUTTON_PRESS =>
                    self.button_press(xev, term),
                x11::BUTTON_RELEASE =>
                    self.button_release(xev, term),
                x11::SELECTION_NOTIFY =>
                    self.selection_notify(xev, term),
                x11::SELECTION_REQUEST =>
                    self.selection_request(xev),
                _ =>
                    println!("event type {:?}", xev_type),
            }
        }
    }

    fn key_press(&mut self, xev: x11::XEvent, term: &mut Term) {
        let mut xev = xev;
        let xev: &mut x11::XKeyEvent = x11::cast_event_mut(&mut xev);
        let mut buf = [0u8; 64];
        let (ksym, mut len) = x11::XLookupString(xev, &mut buf);

        if let Some(function) = find_shortcut(ksym, xev.state) {
            function.execute(self, term);
            return;
        }

        if let Some(key) = map_key(ksym, xev.state, &self.mode) {
            self.term_write(term, &key);
            return;
        }

        if len == 0 {
            return;
        }

        if len == 1 && xev.state & x11::MOD1_MASK != 0 {
            if self.mode.contains(Mode::EIGHTBIT) {
                if buf[0] < 0x7F {
                    buf[0] |= 0x80;
                }
            } else {
                buf[1] = buf[0];
                buf[0] = 0x1B;
                len = 2;
            }
        }
        self.term_write(term, &buf[..len]);
    }

    fn client_message(&mut self, xev: x11::XEvent) {
        let xev: &x11::XClientMessageEvent = x11::cast_event(&xev);
        if xev.message_type == self.wm_protocols && xev.format == 32 {
            let protocol = xev.data.get_long(0) as x11::Atom;
            if protocol == self.wm_delete_window {
                app_exit();
            }
        }
    }

    fn configure_notify(&mut self, xev: x11::XEvent, term: &mut Term) {
        let xev: &x11::XConfigureEvent = x11::cast_event(&xev);
        let cols = xev.width as usize / self.cw;
        let rows = xev.height as usize / self.ch;
        if term.resize(cols, rows) {
            return;
        }

        let width = term.cols * self.cw;
        let height = term.rows * self.ch;
        let depth = x11::XDefaultDepth(self.dpy, self.scr);
        x11::XFreePixmap(self.dpy, self.buf);
        self.buf = x11::XCreatePixmap(self.dpy, self.win, width, height, depth);
        x11::XftDrawChange(self.draw, self.buf);
    }

    fn visibility_notify(&mut self, xev: x11::XEvent) {
        let xev: &x11::XVisibilityEvent = x11::cast_event(&xev);
        self.visible = xev.state != x11::VisibilityFullyObscured;
    }

    fn unmap_notify(&mut self, _xev: x11::XEvent) {
        self.visible = false;
    }

    // FIXME: select rectangle
    fn motion_notify(&mut self, xev: x11::XEvent, term: &mut Term) {
        let xev: &x11::XMotionEvent = x11::cast_event(&xev);
        let (x, y) = self.term_point(xev.x, xev.y);
        term.selection_extend(x, y);
    }

    fn button_press(&mut self, xev: x11::XEvent, term: &mut Term) {
        let xev: &x11::XButtonEvent = x11::cast_event(&xev);
        if xev.button == 1 {
            let (x, y) = self.term_point(xev.x, xev.y);
            term.selection_start(x, y, self.sel_snap.click());
        }
    }

    fn button_release(&mut self, xev: x11::XEvent, term: &mut Term) {
        let xev: &x11::XButtonEvent = x11::cast_event(&xev);
        match xev.button {
            2 => self.selection_paste(),
            1 => self.selection_set(xev.time, term),
            _ => (),
        }
    }

    fn selection_notify(&mut self, xev: x11::XEvent, term: &mut Term) {
        let xev: &x11::XSelectionEvent = x11::cast_event(&xev);
        if xev.property == 0 {
            return;
        }

        let mut ofs = 0;
        let mut nitems = 0;
        let mut rem = 0;
        let mut t = 0;
        let mut format = 0;
        let mut data = null_mut();

        loop {
            if x11::XGetWindowProperty(
                self.dpy, self.win, xev.property, ofs, 1024, 0, 0,
                &mut t, &mut format, &mut nitems, &mut rem, &mut data
            ) != 0 {
                println!("XGetWindowProperty error");
                return;
            }

            if t != self.sel_type {
                println!("returned type {}", t);
                return;
            }

            let len = (nitems * (format as u64) / 8) as usize;
            let buf = unsafe { slice::from_raw_parts(data, len) };
            self.term_write(term, buf);
            x11::XFree(data as *mut _);

            if rem == 0 {
                break;
            } else {
                ofs += (nitems * (format as u64) / 32) as i64;
            }
        }
    }

    fn selection_request(&mut self, xev: x11::XEvent) {
        let xev: &x11::XSelectionRequestEvent = x11::cast_event(&xev);
        let text = self.sel_text.as_ref().ok_or("sel_text is none").unwrap();

        let targets = x11::XInternAtom(self.dpy, "TARGETS", x11::False);
        if xev.target == targets {
            x11::XChangeProperty(
                xev.display, xev.requestor, xev.property, x11::XA_ATOM,
                32, x11::PROP_MODE_REPLACE,
                &self.sel_type as *const _ as *const _, 1
            );
        } else {
            x11::XChangeProperty(
                xev.display, xev.requestor, xev.property, xev.target,
                8, x11::PROP_MODE_REPLACE, text.as_ptr(), text.len()
            );
        }

        let mut xev1 = x11::XSelectionEvent {
            type_: x11::SELECTION_NOTIFY,
            serial: 0,
            send_event: 0,
            display: null_mut(),
            requestor: xev.requestor,
            selection: xev.selection,
            target: xev.target,
            property: xev.property,
            time: xev.time,
        };

        if x11::XSendEvent(
            xev.display, xev.requestor, x11::True, 0,
            &mut xev1 as *mut _ as *mut _
        ) == 0 {
            println!("XSendEvent error");
        }
    }

    fn draw_cells(&self, cs: &[char], prop: GlyphProp, xp: usize, yp: usize) {
        let GlyphProp { mut fg, bg, attr } = prop;
        if attr.contains(GlyphAttr::BOLD) && fg < 8 {
            fg += 8;
        }

        let fg = &self.colors[fg];
        let bg = &self.colors[bg];

        x11::XftDrawRect(self.draw, bg, xp, yp, cs.len()*self.cw, self.ch);
        let idx = cs.iter()
            .map(|&c| x11::XftCharIndex(self.dpy, self.font, c))
            .collect::<Vec<u32>>();
        x11::XftDrawGlyphs(self.draw, fg, self.font, xp, yp+self.ca, &idx);
    }

    fn draw_line(&mut self, term: &mut Term, y: usize) {
        let yp = y * self.ch;
        let mut x0 = 0;
        let mut p0 = term.lines[y][0].prop.resolve(term.selected(0, y));

        for x in x0+1..term.cols {
            let p = term.lines[y][x].prop.resolve(term.selected(x, y));
            if p0 != p {
                let cs = term.lines[y][x0..x].iter()
                    .map(|g| g.c)
                    .collect::<Vec<char>>();
                self.draw_cells(&cs, p0, x0*self.cw, yp);
                x0 = x;
                p0 = p;
            }
        }

        let cs = term.lines[y][x0..term.cols].iter()
            .map(|g| g.c)
            .collect::<Vec<char>>();
        self.draw_cells(&cs, p0, x0*self.cw, yp);
    }

    fn finish_draw(&self, cols: usize, rows: usize) {
        let width = self.cw * cols;
        let height = self.ch * rows;
        x11::XCopyArea(
            self.dpy, self.buf, self.win, self.gc,
            0, 0, width, height,
            0, 0
        );
        x11::XFlush(self.dpy);
    }

    fn term_point(&self, xp: i32, yp: i32) -> (usize, usize) {
        (xp as usize / self.cw, yp as usize / self.ch)
    }

    fn selection_set(&mut self, time: x11::Time, term: &mut Term) {
        self.sel_text = term.selection_get_content();
        if self.sel_text.is_none() {
            return;
        }

        let clipboard = x11::XInternAtom(self.dpy, "CLIPBOARD", x11::False);
        x11::XSetSelectionOwner(self.dpy, clipboard, self.win, time);
        x11::XSetSelectionOwner(self.dpy, x11::XA_PRIMARY, self.win, time);
        if x11::XGetSelectionOwner(self.dpy, clipboard) != self.win ||
            x11::XGetSelectionOwner(self.dpy, x11::XA_PRIMARY) != self.win {
            term.selection_clear();
        }
    }

    pub fn selection_paste(&mut self) {
        x11::XConvertSelection(
            self.dpy, x11::XA_PRIMARY, self.sel_type,
            x11::XA_PRIMARY, self.win, x11::CURRENT_TIME,
        );
    }

    fn term_write(&mut self, term: &mut Term, buf: &[u8]) {
        if self.mode.contains(Mode::ECHO) {
            term.put_string(term_decode(buf));
        }
        term.pty.schedule_write(buf.to_vec());
    }
}

impl Drop for Win {
    fn drop(&mut self) {
        x11::XDestroyWindow(self.dpy, self.win);
        x11::XCloseDisplay(self.dpy);
    }
}
