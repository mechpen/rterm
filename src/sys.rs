extern crate libc;

use std::ptr::{
    null,
    null_mut,
};
use std::ffi::{
    CString,
    CStr,
};
use std::env;
use std::time;
use std::os::raw::*;
use std::convert::TryFrom;
use std::mem::MaybeUninit;
use std::process::exit;
use std::fs::File;
use std::io::prelude::*;
use std::os::unix::io::FromRawFd;

use crate::Result;

fn errno() -> i32 {
    unsafe { *libc::__errno_location() as i32 }
}

extern "C" {
    fn wcwidth(c: libc::wchar_t) -> c_int;
}

pub fn wc_width(c: u32) -> usize {
    let mut w = unsafe { wcwidth(c as libc::wchar_t) };
    if w < 0 {
        w = 1;
    }
    w as usize
}

pub fn setlocale() -> Result<()> {
    let s = CString::new("")?;
    let r = unsafe {libc::setlocale(libc::LC_CTYPE, s.as_ptr()) };
    if r == null_mut() {
        Err("setlocale error".into())
    } else {
        Ok(())
    }
}

extern fn sigchld(_: i32) {
    exit(0);
}

pub fn forkpty() -> Result<(c_int, c_int)> {
    let mut fd = 0;
    let pid = unsafe { libc::forkpty(&mut fd, null_mut(), null(), null()) };
    if pid == -1 {
        return Err(error!("forkpty errno {}", errno()));
    }

    if pid != 0 {
        unsafe { libc::signal(libc::SIGCHLD, sigchld as *mut c_void as usize) };
    }

    Ok((pid, fd))
}

pub fn resizepty(fd: c_int, cols: usize, rows: usize) -> Result<()> {
    let cols = c_ushort::try_from(cols).unwrap();
    let rows = c_ushort::try_from(rows).unwrap();
    let w = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    match unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &w) } {
        -1 => Err(error!("ioctl errno {}", errno())),
        x => Ok(()),
    }
}

pub fn send_sighup(pid: c_int) -> Result<()> {
    match unsafe { libc::kill(pid, libc::SIGHUP) } {
        -1 => Err(error!("kill errno {}", errno())),
        x => Ok(()),
    }
}

fn _execsh() -> Result<()> {
    let pw = unsafe { libc::getpwuid(libc::getuid()) };
    if pw == null_mut() {
        return Err(error!("getpwuid errno {}", errno()));
    }

    let sh = match env::var("SHELL") {
        Ok(v) => v,
        Err(_) => match unsafe { *(*pw).pw_shell } {
            0 => "/bin/sh".into(),
            _ => unsafe {
                CStr::from_ptr((*pw).pw_shell).to_str()?.to_owned()
            },
        }
    };

    env::remove_var("COLUMNS");
    env::remove_var("LINES");
    env::remove_var("TERMCAP");

    env::set_var("SHELL", &sh);
    // FIXME: check TERM support
    env::set_var("TERM", "st-256color");

    unsafe {
        env::set_var("LOGNAME", CStr::from_ptr((*pw).pw_name).to_str()?);
        env::set_var("USER",    CStr::from_ptr((*pw).pw_name).to_str()?);
        env::set_var("HOME",    CStr::from_ptr((*pw).pw_dir).to_str()?);

        libc::signal(libc::SIGCHLD, libc::SIG_DFL);
        libc::signal(libc::SIGHUP,  libc::SIG_DFL);
        libc::signal(libc::SIGINT,  libc::SIG_DFL);
        libc::signal(libc::SIGQUIT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
        libc::signal(libc::SIGALRM, libc::SIG_DFL);
    }

    let sh = CString::new(sh)?;
    let args = &[sh.as_ptr()];
    if unsafe { libc::execvp(sh.as_ptr(), args.as_ptr()) } == -1 {
        return Err(error!("execvp errno {}", errno()));
    }

    Ok(())
}

pub fn execsh() {
    _execsh().unwrap();
    exit(1);
}

pub fn fdset_new() -> libc::fd_set {
    let mut fdset = MaybeUninit::uninit();
    unsafe {
        libc::FD_ZERO(fdset.as_mut_ptr());
        fdset.assume_init()
    }
}

pub fn fdset_set(fdset: &mut libc::fd_set, fd: c_int, maxfd: &mut c_int) {
    unsafe { libc::FD_SET(fd, fdset) };
    if fd > *maxfd {
        *maxfd = fd;
    }
}

pub fn fdset_is_set(fdset: &mut libc::fd_set, fd: c_int) -> bool {
    unsafe {
        libc::FD_ISSET(fd, fdset)
    }
}

pub fn select(
    nfds: c_int,
    rfdset: Option<&mut libc::fd_set>,
    wfdset: Option<&mut libc::fd_set>,
    efdset: Option<&mut libc::fd_set>,
    timeout: Option<time::Duration>,
) -> Result<()> {
    let timeout = timeout.map(|x| {
        libc::timeval {
            tv_sec: x.as_secs() as libc::time_t,
            tv_usec: x.subsec_micros() as libc::suseconds_t,
        }
    });

    let ret = unsafe {
        libc::select(
            nfds,
            rfdset.map_or(null_mut(), |x| x),
            wfdset.map_or(null_mut(), |x| x),
            efdset.map_or(null_mut(), |x| x),
            timeout.map_or(null_mut(), |mut x| &mut x),
        )
    };
    if ret == -1 {
        let err = errno();
        if err != libc::EINTR {
            return Err(error!("select errno {}", err));
        }
    }

    Ok(())
}

pub fn read(fd: c_int, buf: &mut [u8]) -> Result<usize> {
    match unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) } {
        -1 => Err(error!("read errno {}", errno())),
        x => Ok(x as usize),
    }
}

pub fn write(fd: c_int, buf: &[u8]) -> Result<usize> {
    match unsafe { libc::write(fd, buf.as_ptr() as *const _, buf.len()) } {
        -1 => Err(error!("write errno {}", errno())),
        x => Ok(x as usize),
    }
}

pub fn close(fd: c_int) -> Result<()> {
    match unsafe { libc::close(fd) } {
        -1 => Err(error!("close errno {}", errno())),
        x => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wcwidth() {
        super::setlocale().unwrap();
        assert_eq!(super::wc_width('a' as u32).unwrap(), 1);
        assert_eq!(super::wc_width('锈' as u32).unwrap(), 2);
    }

    #[test]
    fn forkpty() {
        let result = super::forkpty();
        assert!(result.is_ok());
    }
}
