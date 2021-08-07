use crate::app::app_exit;
use crate::color::{
    BG_COLOR, BG_COLOR_NAME, CURSOR_COLOR, CURSOR_COLOR_NAME, CURSOR_REV_COLOR,
    CURSOR_REV_COLOR_NAME, FG_COLOR_NAME,
};
use crate::cursor::CursorMode;
use crate::font::Font;
use crate::glyph::{GlyphAttr, GlyphProp};
use crate::keymap::map_key;
use crate::shortcut::find_shortcut;
use crate::snap::Snap;
use crate::term::Term;
use crate::utils::term_decode;
use crate::x11_wrapper as x11;
use crate::{Error, Result};
use bitflags::bitflags;
use std::os::raw::*;
use std::os::unix::io::RawFd;
use std::ptr::null_mut;
use std::slice;

bitflags! {
    pub struct WinMode: u32 {
        const APPKEYPAD   = 1 << 0;
        const REVERSE     = 1 << 1;
        const HIDE        = 1 << 2;
        const APPCURSOR   = 1 << 3;
        const EIGHT_BIT   = 1 << 4;
        const BLINK       = 1 << 5;
        const NUMLOCK     = 1 << 6;
        const ECHO        = 1 << 7;
        const MOUSEBTN    = 1 << 8;
        const MOUSEMOTION = 1 << 9;
        const MOUSESGR    = 1 << 10;
        const MOUSEX10    = 1 << 11;
        const MOUSEMANY   = 1 << 12;
        const MOUSE       = Self::MOUSEBTN.bits|Self::MOUSEMOTION.bits|Self::MOUSEX10.bits|Self::MOUSEMANY.bits;
        const FOCUS       = 1 << 13;
    }
}

// FIXME, this can only be 0 until impemented everywhere.
const BORDERPX: u16 = 0;

/*
 * thickness of underline and bar cursors
 */
const CURSORTHICKNESS: usize = 2;

const FORCEMOUSEMOD: u32 = x11::ShiftMask;

pub struct Win {
    visible: bool,
    mode: WinMode,
    cursor_vis: bool,

    dpy: x11::Display,
    win: x11::Window,
    vis: x11::Visual,
    cmap: x11::Colormap,
    scr: c_int,
    buf: x11::Pixmap,
    gc: x11::GC,
    colors: Vec<x11::XftColor>,
    draw: x11::XftDraw,
    font: Font,
    cw: usize,
    ch: usize,
    ca: usize,
    width: usize,
    height: usize,
    old_mouse_x: usize,
    old_mouse_y: usize,
    old_mouse_button: u32,

    sel_type: x11::Atom,
    sel_snap: Snap,
    sel_text: Option<String>,

    wm_protocols: x11::Atom,
    wm_delete_window: x11::Atom,
    netwmname: x11::Atom,
    netwmiconname: x11::Atom,
    attributes: x11::XSetWindowAttributes,
}

impl Win {
    pub fn new(
        cols: usize,
        rows: usize,
        xoff: usize,
        yoff: usize,
        font: Option<&str>,
    ) -> Result<Self> {
        let dpy = x11::XOpenDisplay()?;
        let scr = x11::XDefaultScreen(dpy);
        let vis = x11::XDefaultVisual(dpy, scr);
        let root = x11::XRootWindow(dpy, scr);

        let font = font.unwrap_or("monospace");
        let font = Font::new(dpy, scr, font)?;
        let (cw, ch) = font.size();
        let ca = font.ascent();
        let (width, height) = (cols * cw, rows * ch);

        let cmap = x11::XDefaultColormap(dpy, scr);
        let mut colors = Vec::with_capacity(260);
        for i in 0..=255 {
            colors.push(
                x11::xloadcolor(dpy, vis, cmap, i, None).expect("Failed to load a default color!"),
            );
        }
        // cursor
        colors.push(
            x11::xloadcolor(dpy, vis, cmap, 256, Some(CURSOR_COLOR_NAME))
                .expect("Failed to load a default color!"),
        );
        // reverse cursor
        colors.push(
            x11::xloadcolor(dpy, vis, cmap, 257, Some(CURSOR_REV_COLOR_NAME))
                .expect("Failed to load a default color!"),
        );
        // foreground
        colors.push(
            x11::xloadcolor(dpy, vis, cmap, 258, Some(FG_COLOR_NAME))
                .expect("Failed to load a default color!"),
        );
        // background
        colors.push(
            x11::xloadcolor(dpy, vis, cmap, 259, Some(BG_COLOR_NAME))
                .expect("Failed to load a default color!"),
        );

        let depth = x11::XDefaultDepth(dpy, scr);
        let attributes_mask = x11::CW_BACK_PIXEL | x11::CW_COLOR_MAP | x11::CW_EVENT_MASK;
        let mut attributes: x11::XSetWindowAttributes = x11::zeroed();
        attributes.colormap = cmap;
        attributes.background_pixel = colors[BG_COLOR].pixel;
        attributes.event_mask = x11::KEY_PRESS_MASK
            | x11::EXPOSURE_MASK
            | x11::VISIBILITY_CHANGE_MASK
            | x11::STRUCTURE_NOTIFY_MASK
            | x11::BUTTON_MOTION_MASK
            | x11::BUTTON_PRESS_MASK
            | x11::BUTTON_RELEASE_MASK;

        let win = x11::XCreateWindow(
            dpy,
            root,
            xoff,
            yoff,
            width,
            height,
            0,
            depth,
            x11::INPUT_OUTPUT,
            vis,
            attributes_mask,
            &mut attributes,
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
        let netwmname = x11::XInternAtom(dpy, "_NET_WM_NAME", x11::False);
        let netwmiconname = x11::XInternAtom(dpy, "_NET_WM_ICON_NAME", x11::False);

        Ok(Win {
            visible: true,
            mode: WinMode::empty(),
            cursor_vis: true,

            sel_type,
            sel_snap: Snap::new(),
            sel_text: None,

            dpy,
            win,
            vis,
            cmap,
            scr,

            gc,
            colors,
            buf,
            draw,
            font,
            cw,
            ch,
            ca,
            width,
            height,
            old_mouse_x: 0,
            old_mouse_y: 0,
            old_mouse_button: 0,

            wm_protocols,
            wm_delete_window,
            netwmname,
            netwmiconname,
            attributes,
        })
    }

    pub fn set_pointer_motion(&mut self, motion: bool) {
        if motion {
            self.attributes.event_mask |= x11::POINTER_MOTION_MASK;
        } else {
            self.attributes.event_mask &= !x11::POINTER_MOTION_MASK;
        }
        x11::XChangeWindowAttributes(self.dpy, self.win, x11::CW_EVENT_MASK, self.attributes);
    }

    pub fn num_colors(&self) -> usize {
        self.colors.len()
    }

    pub fn get_color_osc(&self, idx: usize) -> Result<String> {
        if let Some(col) = self.colors.get(idx) {
            Ok(format!(
                "rgb:{:04x}/{:04x}/{:04x}",
                col.color.red, col.color.green, col.color.blue
            ))
        } else {
            Err(Error {
                msg: "Color index to large.".into(),
            })
        }
    }

    pub fn setcolor(&mut self, idx: u16, name: Option<&str>) -> Result<()> {
        if idx as usize >= self.colors.len() {
            return Err(Error {
                msg: format!(
                    "setcolor: color index {} to large, max {}",
                    idx,
                    self.colors.len()
                ),
            });
        }
        let color = x11::xloadcolor(self.dpy, self.vis, self.cmap, idx, name)?;
        self.colors.push(color);
        let color = self.colors.swap_remove(idx as usize);
        unsafe {
            x11::XftColorFree(self.dpy, self.vis, self.cmap, color);
        }
        Ok(())
    }

    pub fn reset_colors(&mut self) {
        for color in self.colors.drain(..) {
            unsafe {
                // All of the colors in self.colors were allocated by Xft.
                x11::XftColorFree(self.dpy, self.vis, self.cmap, color);
            }
        }
        for i in 0..=255 {
            if let Ok(color) = x11::xloadcolor(self.dpy, self.vis, self.cmap, i, None) {
                self.colors.push(color);
            }
        }
        // cursor
        if let Ok(color) =
            x11::xloadcolor(self.dpy, self.vis, self.cmap, 256, Some(CURSOR_COLOR_NAME))
        {
            self.colors.push(color);
        }
        // reverse cursor
        if let Ok(color) = x11::xloadcolor(
            self.dpy,
            self.vis,
            self.cmap,
            257,
            Some(CURSOR_REV_COLOR_NAME),
        ) {
            self.colors.push(color);
        }
        // foreground
        if let Ok(color) = x11::xloadcolor(self.dpy, self.vis, self.cmap, 258, Some(FG_COLOR_NAME))
        {
            self.colors.push(color);
        }
        // background
        if let Ok(color) = x11::xloadcolor(self.dpy, self.vis, self.cmap, 259, Some(BG_COLOR_NAME))
        {
            self.colors.push(color);
        }
    }

    pub fn seticontitle(&self, title: &str) {
        x11::xseticontitle(self.dpy, self.win, self.netwmiconname, title);
    }

    pub fn settitle(&self, title: &str) {
        x11::xsettitle(self.dpy, self.win, self.netwmname, title);
    }

    pub fn fd(&self) -> RawFd {
        x11::XConnectionNumber(self.dpy)
    }

    pub fn bell(&self) {}

    pub fn set_mode(&mut self, mode: WinMode, val: bool) {
        self.mode.set(mode, val);
    }

    pub fn undraw_cursor(&mut self, term: &Term) {
        let (x, y) = (term.c.x, term.c.y);
        let g = term.get_glyph(x, y);
        self.draw_cells(&[g.c], g.prop, x * self.cw, y * self.ch);
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

        if term.c.blink {
            self.cursor_vis = !self.cursor_vis;
        } else {
            self.cursor_vis = true;
        }
        if !self.mode.contains(WinMode::HIDE) && self.cursor_vis {
            let (x, y) = (term.c.x, term.c.y);
            match term.c.mode {
                CursorMode::Block => {
                    let g = term.get_glyph_at_cursor();
                    self.draw_cells(&[g.c], g.prop, x * self.cw, y * self.ch);
                }
                CursorMode::Underline => {
                    let drawcol = if term.is_selected(x, y) {
                        self.colors[CURSOR_REV_COLOR]
                    } else {
                        self.colors[CURSOR_COLOR]
                    };
                    x11::XftDrawRect(
                        self.draw,
                        &drawcol,
                        BORDERPX as usize + x * self.cw,
                        BORDERPX as usize + (y + 1) * self.ch - CURSORTHICKNESS,
                        self.cw,
                        CURSORTHICKNESS,
                    );
                }
                CursorMode::Bar => {
                    let drawcol = if term.is_selected(x, y) {
                        self.colors[CURSOR_REV_COLOR]
                    } else {
                        self.colors[CURSOR_COLOR]
                    };
                    x11::XftDrawRect(
                        self.draw,
                        &drawcol,
                        BORDERPX as usize + x * self.cw,
                        BORDERPX as usize + y * self.ch,
                        CURSORTHICKNESS,
                        self.ch,
                    );
                }
            }
        }

        self.finish_draw(term.cols, term.rows);
    }

    pub fn redraw(&mut self, term: &mut Term) {
        for y in 0..term.rows {
            term.dirty[y] = true;
        }
        self.draw(term);
    }

    pub fn is_pending(&self) -> bool {
        x11::XPending(self.dpy) > 0
    }

    pub fn process_input(&mut self, term: &mut Term) {
        while x11::XPending(self.dpy) > 0 {
            let mut xev = x11::XNextEvent(self.dpy);
            if x11::XFilterEvent(&mut xev, self.win) == x11::True {
                continue;
            }

            let xev_type = x11::event_type(&xev);
            match xev_type {
                x11::EXPOSE => (),
                x11::KEY_RELEASE => (),
                x11::KEY_PRESS => self.key_press(xev, term),
                x11::CLIENT_MESSAGE => self.client_message(xev),
                x11::CONFIGURE_NOTIFY => self.configure_notify(xev, term),
                x11::VISIBILITY_NOTIFY => self.visibility_notify(xev),
                x11::UNMAP_NOTIFY => self.unmap_notify(),
                x11::MOTION_NOTIFY => self.motion_notify(xev, term),
                x11::BUTTON_PRESS => self.button_press(xev, term),
                x11::BUTTON_RELEASE => self.button_release(xev, term),
                x11::SELECTION_NOTIFY => self.selection_notify(xev, term),
                x11::SELECTION_REQUEST => self.selection_request(xev),
                _ => println!("event type {:?}", xev_type),
            }
        }
    }

    fn limit<T: Ord>(x: T, min: T, max: T) -> T {
        if x < min {
            min
        } else if x > max {
            max
        } else {
            x
        }
    }

    fn evcol(&self, e: &x11::XEvent) -> usize {
        let mut x = unsafe { e.button.x - BORDERPX as i32 };
        x = Self::limit(x, 0, (self.width - 1) as i32);
        x as usize / self.cw
    }

    fn evrow(&self, e: &x11::XEvent) -> usize {
        let mut y = unsafe { e.button.y - BORDERPX as i32 };
        y = Self::limit(y, 0, (self.height - 1) as i32);
        y as usize / self.ch
    }

    fn mousereport(&mut self, e: x11::XEvent, term: &mut Term) {
        let x = self.evcol(&e);
        let y = self.evrow(&e);
        let mut button = unsafe { e.button.button };
        let state = unsafe { e.button.state };
        let btype = unsafe { e.button.type_ };

        /* from urxvt */
        if btype == x11::MOTION_NOTIFY {
            if x == self.old_mouse_x && y == self.old_mouse_y {
                return;
            }
            if !self.mode.contains(WinMode::MOUSEMOTION) && !self.mode.contains(WinMode::MOUSEMANY)
            {
                return;
            }
            /* MOUSE_MOTION: no reporting if no button is pressed */
            if self.mode.contains(WinMode::MOUSEMOTION) && self.old_mouse_button == 3 {
                return;
            }

            button = self.old_mouse_button + 32;
            self.old_mouse_x = x;
            self.old_mouse_y = y;
        } else {
            if !self.mode.contains(WinMode::MOUSESGR) && btype == x11::BUTTON_RELEASE {
                button = 3;
            } else {
                button -= x11::Button1;
                if button >= 7 {
                    button += 128 - 7;
                } else if button >= 3 {
                    button += 64 - 3;
                }
            }
            if btype == x11::BUTTON_PRESS {
                self.old_mouse_button = button;
                self.old_mouse_x = x;
                self.old_mouse_y = y;
            } else if btype == x11::BUTTON_RELEASE {
                self.old_mouse_button = 3;
                /* MODE_MOUSEX10: no button release reporting */
                if self.mode.contains(WinMode::MOUSEX10) {
                    return;
                }
                if button == 64 || button == 65 {
                    return;
                }
            }
        }

        if !self.mode.contains(WinMode::MOUSEX10) {
            button += if (state & x11::ShiftMask) != 0 { 4 } else { 0 }
                + if (state & x11::Mod4Mask) != 0 { 8 } else { 0 }
                + if (state & x11::ControlMask) != 0 {
                    16
                } else {
                    0
                };
        }

        let buf;
        if self.mode.contains(WinMode::MOUSESGR) {
            buf = format!(
                "\x1b[<{};{};{}{}",
                button,
                x + 1,
                y + 1,
                if btype == x11::BUTTON_RELEASE {
                    'm'
                } else {
                    'M'
                }
            );
        } else if x < 223 && y < 223 {
            buf = format!("\x1b[M{}{}{}", 32 + button, 32 + x + 1, 32 + y + 1);
        } else {
            return;
        }

        self.term_write(term, buf.as_bytes());
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
            self.term_write(term, key);
            return;
        }

        if len == 0 {
            return;
        }

        if len == 1 && xev.state & x11::MOD1_MASK != 0 {
            if self.mode.contains(WinMode::EIGHT_BIT) {
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
        if !term.resize(cols, rows) {
            return;
        }

        self.width = term.cols * self.cw;
        self.height = term.rows * self.ch;
        let depth = x11::XDefaultDepth(self.dpy, self.scr);
        x11::XFreePixmap(self.dpy, self.buf);
        self.buf = x11::XCreatePixmap(self.dpy, self.win, self.width, self.height, depth);
        x11::XftDrawChange(self.draw, self.buf);
    }

    fn visibility_notify(&mut self, xev: x11::XEvent) {
        let xev: &x11::XVisibilityEvent = x11::cast_event(&xev);
        self.visible = xev.state != x11::VisibilityFullyObscured;
    }

    fn unmap_notify(&mut self) {
        self.visible = false;
    }

    // FIXME: select rectangle
    fn motion_notify(&mut self, xev: x11::XEvent, term: &mut Term) {
        if self.mode.intersects(WinMode::MOUSE) && unsafe { xev.button.state & FORCEMOUSEMOD } == 0
        {
            self.mousereport(xev, term);
            return;
        }
        let xev: &x11::XMotionEvent = x11::cast_event(&xev);
        let (x, y) = self.term_point(xev.x, xev.y);
        term.extend_selection(x, y);
    }

    fn button_press(&mut self, xev: x11::XEvent, term: &mut Term) {
        if self.mode.intersects(WinMode::MOUSE) && unsafe { xev.button.state & FORCEMOUSEMOD } == 0
        {
            self.mousereport(xev, term);
            return;
        }
        let xev: &x11::XButtonEvent = x11::cast_event(&xev);
        if xev.button == 1 {
            let (x, y) = self.term_point(xev.x, xev.y);
            term.start_selection(x, y, self.sel_snap.click());
        }
    }

    fn button_release(&mut self, xev: x11::XEvent, term: &mut Term) {
        if self.mode.intersects(WinMode::MOUSE) && unsafe { xev.button.state & FORCEMOUSEMOD } == 0
        {
            self.mousereport(xev, term);
            return;
        }
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
                self.dpy,
                self.win,
                xev.property,
                ofs,
                1024,
                0,
                0,
                &mut t,
                &mut format,
                &mut nitems,
                &mut rem,
                &mut data,
            ) != 0
            {
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
        if self.sel_text.is_none() {
            return;
        }

        let xev: &x11::XSelectionRequestEvent = x11::cast_event(&xev);
        let text = self.sel_text.as_ref().unwrap();

        let targets = x11::XInternAtom(self.dpy, "TARGETS", x11::False);
        if xev.target == targets {
            x11::XChangeProperty(
                xev.display,
                xev.requestor,
                xev.property,
                x11::XA_ATOM,
                32,
                x11::PROP_MODE_REPLACE,
                &self.sel_type as *const _ as *const _,
                1,
            );
        } else {
            x11::XChangeProperty(
                xev.display,
                xev.requestor,
                xev.property,
                xev.target,
                8,
                x11::PROP_MODE_REPLACE,
                text.as_ptr(),
                text.len(),
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
            xev.display,
            xev.requestor,
            x11::True,
            0,
            &mut xev1 as *mut _ as *mut _,
        ) == 0
        {
            println!("XSendEvent error");
        }
    }

    fn to_truecolor(&self, col: usize) -> x11::XftColor {
        let colfg = x11::XRenderColor {
            alpha: 0xffff,
            red: ((col & 0xff0000) >> 8) as u16,
            green: (col & 0xff00) as u16,
            blue: ((col & 0xff) << 8) as u16,
        };
        x11::XftColorAllocValue(self.dpy, self.vis, self.cmap, &colfg)
            .expect("Failed to alloc truecolor")
    }

    fn draw_cells(&self, cs: &[char], prop: GlyphProp, xp: usize, yp: usize) {
        let GlyphProp { fg, bg, attr } = prop;
        let charlen = if attr.contains(GlyphAttr::WIDE) {
            cs.len() * 2
        } else {
            cs.len()
        };
        let width = charlen * self.cw;
        let mut fg = if fg & (1 << 24) > 0 {
            // truecolor
            self.to_truecolor(fg)
        } else {
            self.colors[fg]
        };
        let bg = if bg & (1 << 24) > 0 {
            // truecolor
            self.to_truecolor(bg)
        } else {
            self.colors[bg]
        };
        let font = self.font.get(attr);

        if attr.contains(GlyphAttr::INVISIBLE) {
            fg = bg;
        }
        if attr.contains(GlyphAttr::FAINT) {
            let faintfg = x11::XRenderColor {
                alpha: fg.color.alpha,
                red: fg.color.red / 2,
                green: fg.color.green / 2,
                blue: fg.color.blue / 2,
            };
            if let Ok(nfg) = x11::XftColorAllocValue(self.dpy, self.vis, self.cmap, &faintfg) {
                fg = nfg;
            } else {
                println!("Failed to alloc truecolor for FAINT")
            }
        }

        x11::XftDrawRect(self.draw, &bg, xp, yp, width, self.ch);
        let idx = cs
            .iter()
            .map(|&c| x11::XftCharIndex(self.dpy, font, c))
            .collect::<Vec<u32>>();
        x11::XftDrawGlyphs(self.draw, &fg, font, xp, yp + self.ca, &idx);

        /* Render underline and strikethrough. */
        if attr.contains(GlyphAttr::UNDERLINE) {
            let y = yp + self.font.ascent() + 1;
            x11::XftDrawRect(self.draw, &fg, xp, y, width, 1);
        }
        if attr.contains(GlyphAttr::STRUCK) {
            let y = yp + (2 * self.font.ascent() / 3);
            x11::XftDrawRect(self.draw, &fg, xp, y, width, 1);
        }
    }

    fn draw_line(&mut self, term: &mut Term, y: usize) {
        let yp = y * self.ch;
        let mut x0 = 0;
        let mut g0 = term.get_glyph(x0, y);
        let mut cs = vec![g0.c];

        for x in x0 + 1..term.cols {
            let g = term.get_glyph(x, y);
            if g0.prop == g.prop {
                cs.push(g.c);
            } else {
                self.draw_cells(&cs, g0.prop, x0 * self.cw, yp);
                x0 = x;
                g0 = g;
                cs = vec![g0.c];
            }
        }
        self.draw_cells(&cs, g0.prop, x0 * self.cw, yp);
    }

    fn finish_draw(&self, cols: usize, rows: usize) {
        let width = self.cw * cols;
        let height = self.ch * rows;
        x11::XCopyArea(
            self.dpy, self.buf, self.win, self.gc, 0, 0, width, height, 0, 0,
        );
        x11::XFlush(self.dpy);
    }

    fn term_point(&self, xp: i32, yp: i32) -> (usize, usize) {
        (xp as usize / self.cw, yp as usize / self.ch)
    }

    fn selection_set(&mut self, time: x11::Time, term: &mut Term) {
        self.sel_text = term.get_selection_content();
        if self.sel_text.is_none() {
            return;
        }

        let clipboard = x11::XInternAtom(self.dpy, "CLIPBOARD", x11::False);
        x11::XSetSelectionOwner(self.dpy, clipboard, self.win, time);
        x11::XSetSelectionOwner(self.dpy, x11::XA_PRIMARY, self.win, time);
        if x11::XGetSelectionOwner(self.dpy, clipboard) != self.win
            || x11::XGetSelectionOwner(self.dpy, x11::XA_PRIMARY) != self.win
        {
            term.clear_selection();
        }
    }

    pub fn selection_paste(&mut self) {
        x11::XConvertSelection(
            self.dpy,
            x11::XA_PRIMARY,
            self.sel_type,
            x11::XA_PRIMARY,
            self.win,
            x11::CURRENT_TIME,
        );
    }

    fn term_write(&mut self, term: &mut Term, buf: &[u8]) {
        if self.mode.contains(WinMode::ECHO) {
            term.put_string(term_decode(buf));
        }
        term.pty.write(buf.to_vec());
    }
}

impl Drop for Win {
    fn drop(&mut self) {
        x11::XDestroyWindow(self.dpy, self.win);
        x11::XCloseDisplay(self.dpy);
    }
}
