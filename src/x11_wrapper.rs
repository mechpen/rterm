#![allow(non_snake_case)]

use crate::color::COLOR_NAMES;
use crate::{Error, Result};
use fontconfig::fontconfig as fc;
use std::convert::TryInto;
use std::ffi::CStr;
use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use std::ptr::{null, null_mut};
use x11::xft;
use x11::xlib;

pub const XC_XTERM: c_uint = 152;

pub use xlib::XA_ATOM;
pub use xlib::XA_PRIMARY;

pub use xlib::CurrentTime as CURRENT_TIME;

pub use xlib::Atom;
pub use xlib::Colormap;
pub use xlib::Cursor;
pub use xlib::False;
pub use xlib::KeySym;
pub use xlib::Pixmap;
pub use xlib::Time;
pub use xlib::True;
pub use xlib::Window;
pub use xlib::XEvent;
pub use xlib::XGCValues;
pub use xlib::XICCEncodingStyle;
pub use xlib::XSetWindowAttributes;
pub use xlib::XTextProperty;
pub use xlib::XUTF8StringStyle;
pub use xlib::GC;

pub use xlib::GCGraphicsExposures as GC_GRAPHICS_EXPOSURES;
pub use xlib::InputOutput as INPUT_OUTPUT;
pub use xlib::PropModeReplace as PROP_MODE_REPLACE;

pub use xlib::CWBackPixel as CW_BACK_PIXEL;
pub use xlib::CWColormap as CW_COLOR_MAP;
pub use xlib::CWEventMask as CW_EVENT_MASK;

pub use xlib::ButtonMotionMask as BUTTON_MOTION_MASK;
pub use xlib::ButtonPressMask as BUTTON_PRESS_MASK;
pub use xlib::ButtonReleaseMask as BUTTON_RELEASE_MASK;
pub use xlib::ExposureMask as EXPOSURE_MASK;
pub use xlib::KeyPressMask as KEY_PRESS_MASK;
pub use xlib::StructureNotifyMask as STRUCTURE_NOTIFY_MASK;
pub use xlib::VisibilityChangeMask as VISIBILITY_CHANGE_MASK;

pub use xlib::KeyPress as KEY_PRESS;
pub use xlib::KeyRelease as KEY_RELEASE;
pub use xlib::Mod1Mask as MOD1_MASK;
pub use xlib::XKeyEvent;

pub use xlib::ButtonPress as BUTTON_PRESS;
pub use xlib::ButtonRelease as BUTTON_RELEASE;
pub use xlib::XButtonEvent;

pub use xlib::MotionNotify as MOTION_NOTIFY;
pub use xlib::XMotionEvent;

pub use xlib::Expose as EXPOSE;

pub use xlib::VisibilityFullyObscured;
pub use xlib::VisibilityNotify as VISIBILITY_NOTIFY;
pub use xlib::XVisibilityEvent;

pub use xlib::MapNotify as MAP_NOTIFY;
pub use xlib::UnmapNotify as UNMAP_NOTIFY;

pub use xlib::ConfigureNotify as CONFIGURE_NOTIFY;
pub use xlib::XConfigureEvent;

pub use xlib::SelectionRequest as SELECTION_REQUEST;
pub use xlib::XSelectionRequestEvent;

pub use xlib::SelectionNotify as SELECTION_NOTIFY;
pub use xlib::XSelectionEvent;

pub use xlib::ClientMessage as CLIENT_MESSAGE;
pub use xlib::XClientMessageEvent;

pub use x11::xrender::XRenderColor;
pub use xft::XftColor;

pub use fc::FC_SLANT_ITALIC;
pub use fc::FC_SLANT_ROMAN;
pub use fc::FC_WEIGHT_BOLD;

pub type Display = *mut xlib::Display;
pub type Visual = *mut xlib::Visual;
pub type XftFont = *mut xft::XftFont;
pub type XftDraw = *mut xft::XftDraw;
pub type FcPattern = *mut xft::FcPattern;

fn cast<T, V>(v: V) -> T
where
    V: TryInto<T>,
    <V as TryInto<T>>::Error: std::fmt::Debug,
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
    if dpy.is_null() {
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

#[allow(clippy::too_many_arguments)]
pub fn XCreateWindow(
    dpy: Display,
    parent: Window,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    border: usize,
    depth: c_uint,
    class: c_int,
    vis: Visual,
    value_mask: c_ulong,
    values: &mut XSetWindowAttributes,
) -> Window {
    unsafe {
        xlib::XCreateWindow(
            dpy,
            parent,
            cast(x),
            cast(y),
            cast(width),
            cast(height),
            cast(border),
            cast(depth),
            cast(class),
            vis,
            value_mask,
            values,
        )
    }
}

pub fn XStoreName(dpy: Display, win: Window, name: &str) {
    if let Ok(name) = CString::new(name) {
        unsafe {
            xlib::XStoreName(dpy, win, name.as_ptr() as *mut _);
        }
    } else {
        println!("XStoreName {} not a valid c_str.", name);
    }
}

pub fn XCreateGC(dpy: Display, d: c_ulong, value_mask: u32, values: &mut XGCValues) -> GC {
    unsafe { xlib::XCreateGC(dpy, d, cast(value_mask), values) }
}

pub fn XCreatePixmap(
    dpy: Display,
    d: c_ulong,
    width: usize,
    height: usize,
    depth: c_uint,
) -> Pixmap {
    unsafe { xlib::XCreatePixmap(dpy, d, width as u32, height as u32, depth) }
}

pub fn XFreePixmap(dpy: Display, pixmap: c_ulong) {
    unsafe {
        xlib::XFreePixmap(dpy, pixmap);
    }
}

pub fn XFree(data: *mut c_void) {
    unsafe {
        xlib::XFree(data);
    }
}

pub fn XCreateFontCursor(dpy: Display, shape: c_uint) -> Cursor {
    unsafe { xlib::XCreateFontCursor(dpy, shape) }
}

pub fn XDefineCursor(dpy: Display, win: Window, cursor: Cursor) {
    unsafe {
        xlib::XDefineCursor(dpy, win, cursor);
    }
}

pub fn XInternAtom(dpy: Display, name: &str, only_if_exists: c_int) -> Atom {
    let name = CString::new(name).unwrap();
    unsafe { xlib::XInternAtom(dpy, name.as_ptr(), only_if_exists) }
}

pub fn XSetWMProtocols(dpy: Display, win: Window, protocols: &mut [Atom]) {
    unsafe {
        xlib::XSetWMProtocols(dpy, win, protocols.as_mut_ptr(), cast(protocols.len()));
    }
}

pub fn XMapWindow(dpy: Display, win: Window) {
    unsafe {
        xlib::XMapWindow(dpy, win);
    }
}

pub fn XDestroyWindow(dpy: Display, win: Window) {
    unsafe {
        xlib::XDestroyWindow(dpy, win);
    }
}

pub fn XCloseDisplay(dpy: Display) {
    unsafe {
        xlib::XCloseDisplay(dpy);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn XCopyArea(
    dpy: Display,
    src: c_ulong,
    dst: c_ulong,
    gc: GC,
    src_x: usize,
    src_y: usize,
    width: usize,
    height: usize,
    dst_x: usize,
    dst_y: usize,
) {
    unsafe {
        xlib::XCopyArea(
            dpy,
            src,
            dst,
            gc,
            cast(src_x),
            cast(src_y),
            cast(width),
            cast(height),
            cast(dst_x),
            cast(dst_y),
        );
    }
}

pub fn XFlush(dpy: Display) {
    unsafe {
        xlib::XFlush(dpy);
    }
}

pub fn XSync(dpy: Display, discard: c_int) {
    unsafe {
        xlib::XSync(dpy, discard);
    }
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
            event,
            buf.as_mut_ptr() as *mut _,
            cast(buf.len()),
            &mut ksym,
            null_mut(),
        )
    };
    (ksym, cast(len))
}

#[allow(clippy::too_many_arguments)]
pub fn XGetWindowProperty(
    dpy: Display,
    win: Window,
    property: c_ulong,
    ofs: c_long,
    length: c_long,
    delete: c_int,
    req_type: c_ulong,
    ret_type: *mut Atom,
    ret_format: *mut c_int,
    ret_nitems: *mut c_ulong,
    nbytes_more: *mut c_ulong,
    ret_props: *mut *mut c_uchar,
) -> c_int {
    unsafe {
        xlib::XGetWindowProperty(
            dpy,
            win,
            property,
            ofs,
            length,
            delete,
            req_type,
            ret_type,
            ret_format,
            ret_nitems,
            nbytes_more,
            ret_props,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn XChangeProperty(
    dpy: Display,
    win: Window,
    property: c_ulong,
    property_type: c_ulong,
    format: c_int,
    mode: c_int,
    data: *const c_uchar,
    nitems: usize,
) -> c_int {
    unsafe {
        xlib::XChangeProperty(
            dpy,
            win,
            property,
            property_type,
            format,
            mode,
            data,
            cast(nitems),
        )
    }
}

pub fn XSendEvent(
    dpy: Display,
    win: Window,
    propagate: c_int,
    mask: c_long,
    event: *mut XEvent,
) -> c_int {
    unsafe { xlib::XSendEvent(dpy, win, propagate, mask, event) }
}

pub fn XSetSelectionOwner(dpy: Display, selection: Atom, win: Window, time: Time) {
    unsafe {
        xlib::XSetSelectionOwner(dpy, selection, win, time);
    }
}

pub fn XGetSelectionOwner(dpy: Display, selection: Atom) -> Window {
    unsafe { xlib::XGetSelectionOwner(dpy, selection) }
}

pub fn XConvertSelection(
    dpy: Display,
    selection: Atom,
    target: Atom,
    property: Atom,
    requester: Window,
    time: Time,
) {
    unsafe {
        xlib::XConvertSelection(dpy, selection, target, property, requester, time);
    }
}

pub fn XftNameParse(name: &str) -> Result<FcPattern> {
    let name = CString::new(name).unwrap();
    let pattern = unsafe { xft::XftNameParse(name.as_ptr() as *const _) };
    if pattern.is_null() {
        return Err("can't parse font name".into());
    }
    Ok(pattern)
}

pub fn XftFontMatch(dpy: Display, scr: c_int, pattern: FcPattern) -> Result<FcPattern> {
    let mut result = xft::FcResult::NoMatch;
    let pattern = unsafe { xft::XftFontMatch(dpy, scr, pattern, &mut result) };
    if pattern.is_null() {
        return Err("can't match font".into());
    }
    Ok(pattern)
}

pub fn XftFontOpenPattern(dpy: Display, pattern: FcPattern) -> Result<XftFont> {
    let font = unsafe { xft::XftFontOpenPattern(dpy, pattern) };
    if font.is_null() {
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
    dpy: Display,
    vis: Visual,
    cmap: Colormap,
    name_in: &str,
) -> Result<XftColor> {
    let mut col = mem::MaybeUninit::uninit();
    let name = CString::new(name_in).unwrap();
    unsafe {
        if xft::XftColorAllocName(dpy, vis, cmap, name.as_ptr(), col.as_mut_ptr()) == 1 {
            Ok(col.assume_init())
        } else {
            Err(Error {
                msg: format!("Invalid color name: {}", name_in),
            })
        }
    }
}

pub fn XftColorAllocValue(
    dpy: Display,
    vis: Visual,
    cmap: Colormap,
    renderColor: &XRenderColor,
) -> Result<XftColor> {
    let mut col = mem::MaybeUninit::uninit();
    unsafe {
        if xft::XftColorAllocValue(dpy, vis, cmap, renderColor, col.as_mut_ptr()) == 1 {
            Ok(col.assume_init())
        } else {
            Err(Error {
                msg: "Invalid color value".to_string(),
            })
        }
    }
}

/// color MUST have been allocated by Xft (one of the XftColorAlloc... calls).
pub unsafe fn XftColorFree(dpy: Display, vis: Visual, cmap: Colormap, mut color: XftColor) {
    xft::XftColorFree(dpy, vis, cmap, &mut color as *mut XftColor);
}

pub fn XftDrawCreate(dpy: Display, d: c_ulong, vis: Visual, cmap: c_ulong) -> XftDraw {
    unsafe { xft::XftDrawCreate(dpy, d, vis, cmap) }
}

pub fn XftDrawRect(d: XftDraw, color: &XftColor, x: usize, y: usize, width: usize, height: usize) {
    unsafe {
        xft::XftDrawRect(d, color, cast(x), cast(y), cast(width), cast(height));
    }
}

pub fn XftDrawGlyphs(
    d: XftDraw,
    color: &XftColor,
    font: XftFont,
    x: usize,
    y: usize,
    idx: &[c_uint],
) {
    unsafe {
        xft::XftDrawGlyphs(
            d,
            color,
            font,
            cast(x),
            cast(y),
            idx.as_ptr(),
            cast(idx.len()),
        );
    }
}

pub fn XftCharIndex(dpy: Display, font: XftFont, c: char) -> c_uint {
    unsafe { xft::XftCharIndex(dpy, font, cast(c)) }
}

pub fn XftDrawChange(xft_draw: XftDraw, d: c_ulong) {
    unsafe {
        xft::XftDrawChange(xft_draw, d);
    }
}

pub fn FcPatternDestroy(pattern: FcPattern) {
    unsafe { fc::FcPatternDestroy(pattern as _) }
}

pub fn FcPatternDel(pattern: FcPattern, object: &CStr) {
    unsafe {
        fc::FcPatternDel(pattern as _, object.as_ptr());
    }
}

pub fn FcPatternAddInteger(pattern: FcPattern, object: &CStr, i: c_int) {
    unsafe {
        fc::FcPatternAddInteger(pattern as _, object.as_ptr(), i);
    }
}

pub fn xseticontitle(dpy: Display, win: Window, netwmiconname: Atom, title: &str) {
    if let Ok(p) = CString::new(title) {
        let mut pt = p.into_bytes_with_nul();
        let mut p = pt.as_mut_ptr() as *mut c_char;
        unsafe {
            let mut prop = mem::MaybeUninit::uninit();
            let r = xlib::Xutf8TextListToTextProperty(
                dpy,
                &mut p,
                1,
                XUTF8StringStyle,
                prop.as_mut_ptr(),
            );
            if r == xlib::Success as i32 {
                let mut prop = prop.assume_init();
                let prop_ptr = &mut prop as *mut XTextProperty;
                xlib::XSetWMIconName(dpy, win, prop_ptr);
                xlib::XSetTextProperty(dpy, win, prop_ptr, netwmiconname);
                xlib::XFree(prop.value as *mut c_void);
            } else {
                match r {
                    // XLocalNotSupported
                    -2 => println!("error setting icon title '{}': Locale not supported", title),
                    // XConverterNotFound
                    -3 => println!("error setting icon title '{}': Converter Not Found", title),
                    _ => println!("error setting icon title: '{}': unknown code: {}", title, r),
                }
            }
        }
    } else {
        println!("xseticontitle: {} not a valid c_str.", title);
    }
}

pub fn xsettitle(dpy: Display, win: Window, netwmname: Atom, title: &str) {
    if let Ok(p) = CString::new(title) {
        let mut pt = p.into_bytes_with_nul();
        let mut p = pt.as_mut_ptr() as *mut c_char;
        unsafe {
            let mut prop = mem::MaybeUninit::uninit();
            let r = xlib::Xutf8TextListToTextProperty(
                dpy,
                &mut p,
                1,
                XUTF8StringStyle,
                prop.as_mut_ptr(),
            );
            if r == xlib::Success as i32 {
                let mut prop = prop.assume_init();
                let prop_ptr = &mut prop as *mut XTextProperty;
                xlib::XSetWMName(dpy, win, prop_ptr);
                xlib::XSetTextProperty(dpy, win, prop_ptr, netwmname);
                xlib::XFree(prop.value as *mut c_void);
            } else {
                match r {
                    // XLocalNotSupported
                    -2 => println!("error setting title '{}': Locale not supported", title),
                    // XConverterNotFound
                    -3 => println!("error setting title '{}': Converter Not Found", title),
                    _ => println!("error setting title: '{}': unknown code: {}", title, r),
                }
            }
        }
    } else {
        println!("xsettitle: {} not a valid c_str.", title);
    }
}

fn sixd_to_16bit(x: u16) -> u16 {
    if x == 0 {
        0
    } else {
        0x3737 + 0x2828 * x
    }
}

pub fn xloadcolor(
    dpy: Display,
    vis: Visual,
    cmap: Colormap,
    idx: u16,
    name: Option<&str>,
) -> Result<XftColor> {
    if let Some(name) = name {
        XftColorAllocName(dpy, vis, cmap, name)
    } else if (16..=255).contains(&idx) {
        let mut color = XRenderColor {
            red: 0,
            blue: 0,
            green: 0,
            alpha: 0xffff,
        };

        /* 256 color */
        if idx < 6 * 6 * 6 + 16 {
            /* same colors as xterm */
            color.red = sixd_to_16bit(((idx - 16) / 36) % 6);
            color.green = sixd_to_16bit(((idx - 16) / 6) % 6);
            color.blue = sixd_to_16bit((idx - 16) % 6);
        } else {
            /* greyscale */
            color.red = 0x0808 + 0x0a0a * (idx - (6 * 6 * 6 + 16));
            color.green = color.red;
            color.blue = color.red;
        }
        XftColorAllocValue(dpy, vis, cmap, &color)
    } else if let Some(col) = COLOR_NAMES.get(idx as usize) {
        XftColorAllocName(dpy, vis, cmap, col)
    } else {
        Err(Error {
            msg: "Invalid index/name in xloadcolor call!".into(),
        })
    }
}
