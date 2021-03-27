#![allow(non_snake_case)]

use x11::xlib;
use x11::xft;
use std::mem;
use std::os::raw::*;
use std::ffi::CString;
use std::convert::TryInto;
use std::ptr::{
    null,
    null_mut,
};
use crate::Result;

pub const XC_XTERM: c_uint = 152;

pub use xlib::XA_ATOM;
pub use xlib::XA_PRIMARY;

pub use xlib::CurrentTime as CURRENT_TIME;

pub use xlib::Atom;
pub use xlib::Window;
pub use xlib::Colormap;
pub use xlib::Pixmap;
pub use xlib::GC;
pub use xlib::Cursor;
pub use xlib::XSetWindowAttributes;
pub use xlib::XGCValues;
pub use xlib::XEvent;
pub use xlib::KeySym;
pub use xlib::Time;
pub use xlib::True;
pub use xlib::False;

pub use xlib::InputOutput as INPUT_OUTPUT;
pub use xlib::GCGraphicsExposures as GC_GRAPHICS_EXPOSURES;
pub use xlib::PropModeReplace as PROP_MODE_REPLACE;

pub use xlib::CWBackPixel as CW_BACK_PIXEL;
pub use xlib::CWColormap as CW_COLOR_MAP;
pub use xlib::CWEventMask as CW_EVENT_MASK;

pub use xlib::KeyPressMask as KEY_PRESS_MASK;
pub use xlib::ExposureMask as EXPOSURE_MASK;
pub use xlib::VisibilityChangeMask as VISIBILITY_CHANGE_MASK;
pub use xlib::StructureNotifyMask as STRUCTURE_NOTIFY_MASK;
pub use xlib::ButtonMotionMask as BUTTON_MOTION_MASK;
pub use xlib::ButtonPressMask as BUTTON_PRESS_MASK;
pub use xlib::ButtonReleaseMask as BUTTON_RELEASE_MASK;

pub use xlib::XKeyEvent;
pub use xlib::Mod1Mask as MOD1_MASK;
pub use xlib::KeyPress as KEY_PRESS;
pub use xlib::KeyRelease as KEY_RELEASE;

pub use xlib::ButtonPress as BUTTON_PRESS;
pub use xlib::ButtonRelease as BUTTON_RELEASE;
pub use xlib::XButtonEvent;

pub use xlib::MotionNotify as MOTION_NOTIFY;
pub use xlib::XMotionEvent;

pub use xlib::Expose as EXPOSE;

pub use xlib::VisibilityNotify as VISIBILITY_NOTIFY;
pub use xlib::XVisibilityEvent;
pub use xlib::VisibilityFullyObscured;

pub use xlib::UnmapNotify as UNMAP_NOTIFY;
pub use xlib::MapNotify as MAP_NOTIFY;

pub use xlib::ConfigureNotify as CONFIGURE_NOTIFY;
pub use xlib::XConfigureEvent;

pub use xlib::SelectionRequest as SELECTION_REQUEST;
pub use xlib::XSelectionRequestEvent;

pub use xlib::SelectionNotify as SELECTION_NOTIFY;
pub use xlib::XSelectionEvent;

pub use xlib::ClientMessage as CLIENT_MESSAGE;
pub use xlib::XClientMessageEvent;

pub use xft::XftColor;

pub type Display = *mut xlib::Display;
pub type Visual = *mut xlib::Visual;
pub type XftFont = *mut xft::XftFont;
pub type XftDraw = *mut xft::XftDraw;

fn cast<T, V>(v: V) -> T
where V: TryInto<T>, <V as TryInto<T>>::Error: std::fmt::Debug
{
    v.try_into().unwrap()
}

pub fn zeroed<T>() -> T {
    unsafe { mem::zeroed() }
}

pub fn event_type(event: &XEvent) -> c_int {
    unsafe { event.type_ }
}

pub fn cast_event_mut<T>(event: &mut XEvent) -> &mut T {
    unsafe { mem::transmute(event) }
}

pub fn cast_event<T>(event: &XEvent) -> &T {
    unsafe { mem::transmute(event) }
}

pub fn XOpenDisplay() -> Result<Display> {
    let dpy = unsafe { xlib::XOpenDisplay(null()) };
    if dpy == null_mut() {
        return Err("can't open display".into());
    }
    Ok(dpy)
}

pub fn XDefaultScreen(dpy: Display) -> c_int {
    unsafe { xlib::XDefaultScreen(dpy) }
}

pub fn XDefaultVisual(dpy: Display, scr: c_int) -> Visual {
    unsafe { xlib::XDefaultVisual(dpy, scr) }
}

pub fn XDefaultColormap(dpy: Display, scr: c_int) -> Colormap {
    unsafe { xlib::XDefaultColormap(dpy, scr) }
}

pub fn XDefaultDepth(dpy: Display, scr: c_int) -> c_uint {
    unsafe { cast(xlib::XDefaultDepth(dpy, scr)) }
}

pub fn XRootWindow(dpy: Display, scr: c_int) -> Window {
    unsafe { xlib::XRootWindow(dpy, scr) }
}

pub fn XCreateWindow(
    dpy: Display, parent: Window,
    x: usize, y :usize, width: usize, height: usize,
    border: usize, depth: c_uint, class: c_int, vis: Visual,
    value_mask: c_ulong, values: &mut XSetWindowAttributes,
) -> Window {
    unsafe {
        xlib::XCreateWindow(
            dpy, parent,
            cast(x), cast(y), cast(width), cast(height),
            cast(border), cast(depth), cast(class), vis,
            value_mask, values
        )
    }
}

pub fn XStoreName(dpy: Display, win: Window, name: &str) {
    let name = CString::new(name).unwrap();
    unsafe { xlib::XStoreName(dpy, win, name.as_ptr() as *mut _); }
}

pub fn XCreateGC(
    dpy: Display, d: c_ulong, value_mask: u32, values: &mut XGCValues
) -> GC {
    unsafe { xlib::XCreateGC(dpy, d, cast(value_mask), values) }
}

pub fn XCreatePixmap(
    dpy: Display, d: c_ulong, width: usize, height: usize, depth: c_uint
) -> Pixmap {
    unsafe { xlib::XCreatePixmap(dpy, d, width as u32, height as u32, depth) }
}

pub fn XFreePixmap(dpy: Display, pixmap: c_ulong) {
    unsafe { xlib::XFreePixmap(dpy, pixmap); }
}

pub fn XFree(data: *mut c_void) {
    unsafe { xlib::XFree(data); }
}

pub fn XCreateFontCursor(dpy: Display, shape: c_uint) -> Cursor {
    unsafe { xlib::XCreateFontCursor(dpy, shape) }
}

pub fn XDefineCursor(dpy: Display, win: Window, cursor: Cursor) {
    unsafe { xlib::XDefineCursor(dpy, win, cursor); }
}

pub fn XInternAtom(dpy: Display, name: &str, only_if_exists: c_int) -> Atom {
    let name = CString::new(name).unwrap();
    unsafe { xlib::XInternAtom(dpy, name.as_ptr(), only_if_exists) }
}

pub fn XSetWMProtocols(dpy: Display, win: Window, protocols: &mut [Atom]) {
    unsafe {
        xlib::XSetWMProtocols(
            dpy, win, protocols.as_mut_ptr(), cast(protocols.len())
        );
    }
}

pub fn XMapWindow(dpy: Display, win: Window) {
    unsafe { xlib::XMapWindow(dpy, win); }
}

pub fn XDestroyWindow(dpy: Display, win: Window) {
    unsafe { xlib::XDestroyWindow(dpy, win); }
}

pub fn XCloseDisplay(dpy: Display) {
    unsafe { xlib::XCloseDisplay(dpy); }
}

pub fn XCopyArea(
    dpy: Display, src: c_ulong, dst: c_ulong, gc: GC,
    src_x: usize, src_y: usize, width: usize, height: usize,
    dst_x: usize, dst_y: usize,
) {
    unsafe {
        xlib::XCopyArea(
            dpy, src, dst, gc,
            cast(src_x), cast(src_y), cast(width), cast(height),
            cast(dst_x), cast(dst_y),
        );
    }
}

pub fn XFlush(dpy: Display) {
    unsafe { xlib::XFlush(dpy); }
}

pub fn XSync(dpy: Display, discard: c_int) {
    unsafe { xlib::XSync(dpy, discard); }
}

pub fn XNextEvent(dpy: Display) -> XEvent {
    unsafe {
        let mut event: XEvent = mem::zeroed();
        xlib::XNextEvent(dpy, &mut event);
        event
    }
}

pub fn XFilterEvent(event: &mut XEvent, window: Window) -> c_int {
    unsafe { xlib::XFilterEvent(event, window) }
}

pub fn XPending(dpy: Display) -> c_int {
    unsafe { xlib::XPending(dpy) }
}

pub fn XConnectionNumber(dpy: Display) -> c_int {
    unsafe { xlib::XConnectionNumber(dpy) }
}

pub fn XLookupString(event: &mut XKeyEvent, buf: &mut [u8]) -> (KeySym, usize) {
    let mut ksym: KeySym = 0;
    let len = unsafe {
        xlib::XLookupString(
            event, buf.as_mut_ptr() as *mut _, cast(buf.len()),
            &mut ksym, null_mut()
        )
    };
    (ksym, cast(len))
}

pub fn XGetWindowProperty(
    dpy: Display, win: Window, property: c_ulong,
    ofs: c_long, length: c_long, delete: c_int, req_type: c_ulong,
    ret_type: *mut Atom, ret_format: *mut c_int, ret_nitems: *mut c_ulong,
    nbytes_more: *mut c_ulong, ret_props: *mut *mut c_uchar
) -> c_int {
    unsafe {
        xlib::XGetWindowProperty(
            dpy, win, property, ofs, length, delete, req_type,
            ret_type, ret_format, ret_nitems, nbytes_more, ret_props,
        )
    }
}

pub fn XChangeProperty(
    dpy: Display, win: Window, property: c_ulong, property_type: c_ulong,
    format: c_int, mode: c_int, data: *const c_uchar, nitems: usize,
) -> c_int {
    unsafe {
        xlib::XChangeProperty(
            dpy, win, property, property_type, format, mode, data, cast(nitems),
        )
    }
}

pub fn XSendEvent(
    dpy: Display, win: Window, propagate: c_int,
    mask: c_long, event: *mut XEvent
) -> c_int {
    unsafe {
        xlib::XSendEvent(dpy, win, propagate, mask, event)
    }
}

pub fn XSetSelectionOwner(
    dpy: Display, selection: Atom, win: Window, time: Time
) {
    unsafe { xlib::XSetSelectionOwner(dpy, selection, win, time); }
}

pub fn XGetSelectionOwner(dpy: Display, selection: Atom) -> Window {
    unsafe { xlib::XGetSelectionOwner(dpy, selection) }
}

pub fn XConvertSelection(
    dpy: Display, selection: Atom, target: Atom,
    property: Atom, requester: Window, time: Time
) {
    unsafe {
        xlib::XConvertSelection(
            dpy, selection, target, property, requester, time
        );
    }
}

pub fn XftFontOpenName(
    dpy: Display, scr: c_int, name: &str
) -> Result<XftFont> {
    let name = CString::new(name).unwrap();
    let font = unsafe {
        xft::XftFontOpenName(dpy, scr, name.as_ptr() as *mut _)
    };
    if font == null_mut() {
        return Err("can't load font".into());
    }
    Ok(font)
}

pub fn font_size(font: XftFont) -> (usize, usize) {
    unsafe { (cast((*font).max_advance_width), cast((*font).height)) }
}

pub fn font_ascent(font: XftFont) -> usize {
    unsafe { cast((*font).ascent) }
}

pub fn XftColorAllocName(
    dpy: Display, vis: Visual, cmap: Colormap, name: &str
) -> XftColor {
    let mut col = mem::MaybeUninit::uninit();
    let name = CString::new(name).unwrap();
    unsafe {
        xft::XftColorAllocName(dpy, vis, cmap, name.as_ptr(), col.as_mut_ptr());
        col.assume_init()
    }
}

pub fn XftDrawCreate(
    dpy: Display, d: c_ulong, vis: Visual, cmap: c_ulong
) -> XftDraw {
    unsafe { xft::XftDrawCreate(dpy, d, vis, cmap) }
}

pub fn XftDrawRect(
    d: XftDraw, color: &XftColor,
    x: usize, y: usize, width: usize, height: usize
) {
    unsafe {
        xft::XftDrawRect(d, color, cast(x), cast(y), cast(width), cast(height));
    }
}

pub fn XftDrawGlyphs(
    d: XftDraw, color: &XftColor, font: XftFont,
    x: usize, y: usize, idx: &[c_uint]
) {
    unsafe {
        xft::XftDrawGlyphs(
            d, color, font, cast(x), cast(y), idx.as_ptr(), cast(idx.len())
        );
    }
}

pub fn XftCharIndex(dpy: Display, font: XftFont, c: char) -> c_uint {
    unsafe { xft::XftCharIndex(dpy, font, cast(c)) }
}

pub fn XftDrawChange(xft_draw: XftDraw, d: c_ulong) {
    unsafe { xft::XftDrawChange(xft_draw, d); }
}
