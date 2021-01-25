// FIXME: border

extern crate x11;
use x11::{
    xlib::*,
    xft::*,
    keysym::*,
};

use std::ptr::{
    null,
    null_mut,
};
use std::mem;
use std::slice;
use std::ffi::CString;
use std::os::raw::*;
use std::convert::TryFrom;

use crate::{
    utils::{
        is_set,
        limit,
    },
    term::{
        Term,
        Glyph,
        ATTR_BOLD,
        ATTR_REVERSE,
        ATTR_BOLD_FAINT,
    },
    snap::SnapClick,
    sys,
    utf8,
    keymap,
    Result,
};

const colorname: &[&str] = &[
    /* 8 normal colors */
    "black",
    "red3",
    "green3",
    "yellow3",
    "blue2",
    "magenta3",
    "cyan3",
    "gray90",
    /* 8 bright colors */
    "gray50",
    "red",
    "green",
    "yellow",
    "#5c5cff",
    "magenta",
    "cyan",
    "white",
];

const XC_xterm: c_uint = 152;

const MODE_VISIBLE: u32 = 1 << 0;
const MODE_FOCUSED: u32 = 1 << 1;
const MODE_APPKEYPAD: u32 = 1 << 2;
const MODE_MOUSEBTN: u32 = 1 << 3;
const MODE_MOUSEMOTION: u32 = 1 << 4;
const MODE_REVERSE: u32 = 1 << 5;
const MODE_KBDLOCK: u32 = 1 << 6;
const MODE_HIDE: u32 = 1 << 7;
const MODE_APPCURSOR: u32 = 1 << 8;
const MODE_MOUSESGR: u32 = 1 << 9;
const MODE_8BIT: u32 = 1 << 10;
const MODE_BLINK: u32 = 1 << 11;
const MODE_FBLINK: u32 = 1 << 12;
const MODE_FOCUS: u32 = 1 << 13;
const MODE_MOUSEX10: u32 = 1 << 14;
const MODE_MOUSEMANY: u32 = 1 << 15;
const MODE_BRCKTPASTE: u32 = 1 << 16;
const MODE_NUMLOCK: u32 = 1 << 17;
const MODE_MOUSE: u32 = MODE_MOUSEBTN|MODE_MOUSEMOTION|MODE_MOUSEX10|MODE_MOUSEMANY;

pub struct XWindow {
    dpy:  *mut Display,
    win:  Window,
    scr:  c_int,
    vis:  *mut Visual,

    gc:     GC,
    cmap:   Colormap,
    colors: Vec<XftColor>,

    font: *mut XftFont,
    buf:  Pixmap,
    draw: *mut XftDraw,

    ch:     usize,
    cw:     usize,
    term:   Term,

    seltype: Atom,
    seltext: Option<Vec<u8>>,
    selsnap: SnapClick,

    mode:    u32,
    running: bool,
}

impl XWindow {
    pub fn new(cols: usize, rows: usize, fg: u8, bg: u8) -> Result<Self> {
        // FIXME: new term first
        unsafe {
            let dpy = XOpenDisplay(null());
            if dpy == null_mut() {
                return Err("can't open display".into());
            }

            let scr = XDefaultScreen(dpy);
            let vis = XDefaultVisual(dpy, scr);
            let root = XRootWindow(dpy, scr);

            let s = CString::new("monospace")?;
            let font = XftFontOpenName(dpy, scr, s.as_ptr() as *mut _);
            if font == null_mut() {
                return Err("can't load font".into());
            }
            let cw = (*font).max_advance_width as usize;
            let ch = (*font).height as usize;
            let width = cols * cw;
            let height = rows * ch;

            let cmap = XDefaultColormap(dpy, scr);
            let mut colors = Vec::with_capacity(colorname.len());
            for &name in colorname {
                let mut col = mem::MaybeUninit::uninit();
                let s = CString::new(name)?;
                XftColorAllocName(
                    dpy, vis, cmap, s.as_ptr(), col.as_mut_ptr()
                );
                colors.push(col.assume_init());
            }

            let mut attributes: XSetWindowAttributes = mem::zeroed();
            attributes.colormap = cmap;
            attributes.background_pixel = colors[bg as usize].pixel;
            attributes.event_mask = KeyPressMask | ExposureMask
                | VisibilityChangeMask | StructureNotifyMask
                | ButtonMotionMask | ButtonPressMask | ButtonReleaseMask;

            let win = XCreateWindow(
                dpy, root,
                0, 0, width as c_uint, height as c_uint,
                0,
                XDefaultDepth(dpy, scr),
                InputOutput as c_uint,
                vis,
                CWBackPixel | CWColormap | CWEventMask,
                &mut attributes,
            );

            let s = CString::new("rt")?;
            XStoreName(dpy, win, s.as_ptr() as *mut _);

            let mut gcvalues: XGCValues = mem::zeroed();
            gcvalues.graphics_exposures = False;
            let gc = XCreateGC(dpy, root, GCGraphicsExposures as u64, &mut gcvalues);
            let buf = XCreatePixmap(
                dpy, win,
                width as c_uint, height as c_uint,
                XDefaultDepth(dpy, scr) as u32,
            );
            let draw = XftDrawCreate(dpy, buf, vis, cmap);

            let cursor = XCreateFontCursor(dpy, XC_xterm);
            XDefineCursor(dpy, win, cursor);

            let s = CString::new("WM_DELETE_WINDOW").unwrap();
            let wm_delete_window = XInternAtom(dpy, s.as_ptr(), False);
            let s = CString::new("WM_PROTOCOLS").unwrap();
            let wm_protocols = XInternAtom(dpy, s.as_ptr(), False);
            let mut protocols = [wm_delete_window];
            XSetWMProtocols(dpy, win, &mut protocols[0] as *mut _, 1);
            
            let s = CString::new("UTF8_STRING").unwrap();
            let seltype = XInternAtom(dpy, s.as_ptr(), False);

            XMapWindow(dpy, win);
            XSync(dpy, False);

            Ok(XWindow{
                dpy,
                win,
                scr,
                vis,

                gc,
                cmap,
                colors,

                font,
                buf,
                draw,

                cw,
                ch,
                term: Term::new(cols, rows, fg, bg)?,

                seltype: seltype,
                seltext: None,
                selsnap: SnapClick::new(),

                mode: 0,
                running: true,
            })
        }
    }

    pub fn run(&mut self) -> Result<()> {
        unsafe {
            let mut event: XEvent = mem::zeroed();
            while self.running {
                XNextEvent(self.dpy, &mut event);
                if XFilterEvent(&mut event, 0) == 1 {
                    continue;
                }

                match event.type_ {
                    MapNotify => break,
                    _ => (),
                }
            }

            while self.running {
                let xfd = XConnectionNumber(self.dpy);
                let ptyfd = self.term.get_ptyfd();

                let mut maxfd = 0;
                let mut rfdset = sys::fdset_new();
                sys::fdset_set(&mut rfdset, xfd, &mut maxfd);
                sys::fdset_set(&mut rfdset, ptyfd, &mut maxfd);

                sys::select(
                    maxfd+1, Some(&mut rfdset), None, None, None
                )?;

                if sys::fdset_is_set(&mut rfdset, ptyfd) {
                    self.term.ttyread()?;
                }

                while XPending(self.dpy) > 0 {
                    XNextEvent(self.dpy, &mut event);
                    if XFilterEvent(&mut event, 0) == 1 {
                        continue;
                    }

                    // FIXME: function lookup table in rust
                    match event.type_ {
                        KeyPress => self.kpress(&mut event.key)?,
                        ClientMessage => self.cmessage(&event.client_message)?,
                        ConfigureNotify => self.resize(&event.configure)?,
                        VisibilityNotify => self.visibility(&event.visibility)?,
                        UnmapNotify => self.unmap(&event.unmap)?,
                        Expose => self.expose(&event.expose)?,
                        MotionNotify => self.bmotion(&event.motion)?,
                        ButtonPress => self.bpress(&event.button)?,
                        ButtonRelease => self.brelease(&event.button)?,
                        SelectionNotify => self.selnotify(&event.selection)?,
                        SelectionRequest => self.selrequest(&event.selection_request)?,
                        _ => println!("event type {}", event.type_),
                    }
                }

                self.draw();
            }
        }

        return Ok(())
    }

    fn xfinishdraw(&self) {
        let (cols, rows) = self.term.size();
        let width = (self.cw * cols) as c_uint;
        let height = (self.ch * rows) as c_uint;
        unsafe {
            XCopyArea(
                self.dpy, self.buf, self.win, self.gc,
                0, 0, width, height,
                0, 0
            );
            XFlush(self.dpy);
        }
    }

    // FIXME: optimize by drawing blocks of the same attr
    fn xdrawglyph(&self, g: &Glyph, winx: usize, winy: usize, reverse: bool) {
        let (mut fg, mut bg) = (g.fg, g.bg);

        if is_set(g.attr, ATTR_REVERSE) ^ reverse {
            mem::swap(&mut fg, &mut bg);
        }
        if g.attr & ATTR_BOLD_FAINT == ATTR_BOLD && g.fg < 8 {
            fg += 8;
        }

        let fg = &self.colors[fg as usize];
        let bg = &self.colors[bg as usize];

        unsafe {
            XftDrawRect(
                self.draw, bg, winx as c_int, winy as c_int,
                self.cw as c_uint, self.ch as c_uint
            );
            let idx = XftCharIndex(self.dpy, self.font, g.u);
            XftDrawGlyphs(
                self.draw, fg, self.font,
                winx as c_int, winy as c_int+(*self.font).ascent,
                &idx, 1
            );
        }
    }

    fn xdrawcursor(&mut self) {
        let c = self.term.get_cursor();
        let lines = self.term.get_lines();

        let (ox, oy) = self.term.get_last_pos();
        let reverse = self.term.selected(ox, oy);
        let g = &lines[oy][ox];
        self.xdrawglyph(g, ox*self.cw, oy*self.ch, reverse);

        let mut g = lines[c.y][c.x];
        let reverse = !self.term.selected(ox, oy);
        self.xdrawglyph(&g, c.x*self.cw, c.y*self.ch, reverse);

        self.term.sync_last_pos();
    }

    fn xdrawline(&self, y: usize) {
        let yp = y * self.ch;
        let lines = self.term.get_lines();
        let (cols, _) = self.term.size();

        for x in 0..cols {
            let xp = x * self.cw;
            let g = &lines[y][x];
            self.xdrawglyph(g, xp, yp, self.term.selected(x, y));
        }
    }

    fn draw(&mut self) {
        if !is_set(self.mode, MODE_VISIBLE) {
            return;
        }

        let (_, rows) = self.term.size();
        for y in 0..rows {
            if !self.term.get_dirty(y) {
                continue;
            }
            self.xdrawline(y);
            self.term.set_dirty(y, false);
        }

        self.xdrawcursor();
        self.xfinishdraw();
    }

    fn cmessage(&mut self, event: &XClientMessageEvent) -> Result<()> {
        unsafe {
            let s = CString::new("WM_DELETE_WINDOW").unwrap();
            let wm_delete_window = XInternAtom(self.dpy, s.as_ptr(), False);
            let s = CString::new("WM_PROTOCOLS").unwrap();
            let wm_protocols = XInternAtom(self.dpy, s.as_ptr(), False);

            if event.message_type == wm_protocols && event.format == 32 {
                let protocol = event.data.get_long(0) as Atom;
                if protocol == wm_delete_window {
                    self.running = false;
                    return Ok(());
                }
            }

            return Err("invalid client message".into());
        }
    }

    fn kpress(&mut self, event: &mut XKeyEvent) -> Result<()> {
        unsafe {
            let mut ksym: u64 = 0;
            let mut buf = [0u8; 4];
            let len = XLookupString(
                event, buf.as_mut_ptr() as *mut _, 4, &mut ksym, null_mut()
            );

            if ksym as u32 == XK_Insert && is_set(event.state, ShiftMask) {
                return self.selpaste();
            }

            if let Some(customkey) = self.kmap(ksym as u32, event.state) {
                self.term.ttywrite(customkey, true)?;
                return Ok(());
            }

            if len <= 0 {
                return Ok(());
            }
            let mut len = len as usize;

            if len == 1 && event.state & Mod1Mask != 0 {
                if is_set(self.mode, MODE_8BIT) {
                    if buf[0] < 0o177 {
                        let c = buf[0] | 0x80;
                        len = utf8::encode(c as u32, &mut buf);
                    }
                } else {
                    buf[1] = buf[0];
                    buf[0] = 0o33;
                    len = 2;
                }
            }
            self.term.ttywrite(&buf[..len], true)?;
        }

        Ok(())
    }

    fn resize(&mut self, event: &XConfigureEvent) -> Result<()> {
        let cols = event.width as usize / self.cw;
        let rows = event.height as usize / self.ch;

        if !self.term.tresize(cols, rows) {
            return Ok(());
        }

        let (cols, rows) = self.term.size();
        let width = cols * self.cw;
        let height = rows * self.ch;

        unsafe {
            XFreePixmap(self.dpy, self.buf);
            self.buf = XCreatePixmap(
                self.dpy, self.win,
                width as u32, height as u32,
                XDefaultDepth(self.dpy, self.scr) as u32,
            );
            XftDrawChange(self.draw, self.buf);
        }

        Ok(())
    }

    fn visibility(&mut self, event: &XVisibilityEvent) -> Result<()> {
        if event.state == VisibilityFullyObscured {
            self.mode &= !MODE_VISIBLE;
        } else {
            self.mode |= MODE_VISIBLE;
        }

        Ok(())
    }

    fn unmap(&mut self, event: &XUnmapEvent) -> Result<()> {
        self.mode &= !MODE_VISIBLE;
        Ok(())
    }

    fn expose(&mut self, event: &XExposeEvent) -> Result<()> {
        self.draw();
        Ok(())
    }

    fn evpoint(&self, x: i32, y: i32) -> (i32, i32) {
        (x / self.cw as i32, y /self.ch as i32)
    }

    // FIXME: select rectangle
    fn bmotion(&mut self, event: &XMotionEvent) -> Result<()> {
        let (x, y) = self.evpoint(event.x, event.y);
        self.term.selextend(x, y);
        Ok(())
    }

    fn bpress(&mut self, event: &XButtonEvent) -> Result<()> {
        if event.button == 1 {
            let snap = self.selsnap.click();
            let (x, y) = self.evpoint(event.x, event.y);
            self.term.selstart(x, y, snap);
        }
        Ok(())
    }

    fn brelease(&mut self, event: &XButtonEvent) -> Result<()> {
        match event.button {
            2 => self.selpaste(),
            1 => self.setsel(event.time),
            _ => Ok(()),
        }
    }

    fn selnotify(&mut self, event: &XSelectionEvent) -> Result<()> {
        if event.property == 0 {
            return Ok(())
        }

        let mut ofs = 0;
        let mut nitems = 0;
        let mut rem = 0;
        let mut t = 0;
        let mut format = 0;
        let mut data = null_mut();

        loop {
            unsafe {
                if XGetWindowProperty(
                    self.dpy, self.win, event.property, ofs, 1024, 0, 0,
                    &mut t, &mut format, &mut nitems, &mut rem, &mut data
                ) != 0 {
                    println!("XGetWindowProperty error");
                    return Ok(());
                }
            }

            if t != self.seltype {
                println!("returned type {}", t);
            }

            let len = (nitems * (format as u64) / 8) as usize;
            let buf = unsafe { slice::from_raw_parts(data, len) };
            self.term.ttywrite(buf, true);
            unsafe { XFree(data as *mut _) };

            if rem == 0 {
                break;
            } else {
                ofs += (nitems * (format as u64) / 32) as i64;
            }
        }

        Ok(())
    }

    fn selrequest(&self, event: &XSelectionRequestEvent) -> Result<()> {
        let text = self.seltext.as_ref().ok_or("seltext is none")?;

        unsafe {
            let s = CString::new("TARGETS")?;
            let targets = XInternAtom(self.dpy, s.as_ptr(), 0);
            if event.target == targets {
                XChangeProperty(
                    event.display, event.requestor,
                    event.property, XA_ATOM, 32, PropModeReplace,
                    &self.seltype as *const _ as *const _, 1,
                );
            } else {
                XChangeProperty(
                    event.display, event.requestor,
                    event.property, event.target, 8, PropModeReplace,
                    text.as_ptr(), text.len() as i32,
                );
            }

            let mut xev = XSelectionEvent {
                type_: SelectionNotify,
                serial: 0,
                send_event: 0,
                display: null_mut(),
                requestor: event.requestor,
                selection: event.selection,
                target: event.target,
                property: event.property,
                time: event.time,
            };

            if XSendEvent(
                event.display, event.requestor, 1, 0,
                &mut xev as *mut _ as *mut _,
            ) == 0 {
                println!("XSendEvent error");
            }
        }

        Ok(())
    }

    fn setsel(&mut self, time: Time) -> Result<()> {
        self.seltext = self.term.getsel();

        if self.seltext.is_some() {
            unsafe {
                let s = CString::new("CLIPBOARD")?;
                let clipboard = XInternAtom(self.dpy, s.as_ptr(), 0);

                XSetSelectionOwner(self.dpy, clipboard, self.win, time);
                XSetSelectionOwner(self.dpy, XA_PRIMARY, self.win, time);
                if (XGetSelectionOwner(self.dpy, clipboard) != self.win ||
                    XGetSelectionOwner(self.dpy, XA_PRIMARY) != self.win) {
                    self.term.selclear();
                    return Ok(());
                }


            }
        }

        Ok(())
    }

    fn selpaste(&mut self) -> Result<()> {
        unsafe {
            XConvertSelection(
                self.dpy, XA_PRIMARY, self.seltype,
                XA_PRIMARY, self.win, CurrentTime,
            );
        }
        Ok(())
    }

    pub fn kmap(&self, k: u32, state: c_uint) -> Option<&'static [u8]> {
        if k & 0xFFFF < 0xFD00 {
            return None;
        }

        for key in keymap::keys {
            if key.k != k {
                continue;
            }
            if !keymap::match_mask(key.mask, state) {
                continue;
            }

            if is_set(self.mode, MODE_APPKEYPAD) {
                if key.appkey < 0 {
                    continue;
                }
            } else {
                if key.appkey > 0 {
                    continue;
                }
            }

            if is_set(self.mode, MODE_NUMLOCK) && key.appkey == 2 {
                continue;
            }

            if is_set(self.mode, MODE_APPCURSOR) {
                if key.appcursor < 0 {
                    continue;
                }
            } else {
                if key.appcursor > 0 {
                    continue;
                }
            }

            return Some(key.s);
        }

        None
    }
}

impl Drop for XWindow {
    fn drop(&mut self) {
        unsafe {
            XDestroyWindow(self.dpy, self.win);
            XCloseDisplay(self.dpy);
        }
    }
}
