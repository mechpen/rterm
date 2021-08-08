use crate::term::Term;
use crate::utils::parse_geometry;
use crate::vte::Vte;
use crate::win::Win;
use crate::Result;
use nix;
use nix::errno::Errno;
use nix::sys::select::{select, FdSet};
use nix::sys::signal::{signal, SigHandler, Signal};
use nix::sys::time::{TimeVal, TimeValLike};
use std::fs::File;
use std::io::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

static RUNNING: AtomicBool = AtomicBool::new(true);

fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

pub fn app_exit() {
    RUNNING.store(false, Ordering::Relaxed);
}

fn set_sigchld() {
    extern "C" fn handle_sigchld(_signal: i32) {
        app_exit();
    }
    let handler = SigHandler::Handler(handle_sigchld);
    unsafe {
        signal(Signal::SIGCHLD, handler).unwrap();
    }
}

// Data flow:
//
//   read pty fd --> vte parse --+--> write to pty fd
//                               |
//                               +--> update term
//                               |
//        win input -------------+--> update win

pub struct App {
    term: Term,
    win: Win,
    vte: Vte,
    log: Option<File>,
}

impl App {
    pub fn new(geometry: Option<&str>, font: Option<&str>, log: Option<&str>) -> Result<Self> {
        let log = match log {
            Some(x) => Some(File::create(x)?),
            None => None,
        };
        let (cols, rows, xoff, yoff) = match geometry {
            Some(x) => parse_geometry(x)?,
            None => (80, 24, 0, 0),
        };

        set_sigchld();

        let term = Term::new(cols, rows)?;
        Ok(App {
            win: Win::new(term.cols, term.rows, xoff, yoff, font)?,
            term,
            vte: Vte::new(),
            log,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let win_fd = self.win.fd();
        let pty_fd = self.term.pty.fd();
        let mut buf = [0; 8192];
        let mut last_blink = SystemTime::now();
        let blink_duration = Duration::from_millis(500);

        while is_running() {
            let blink_elapsed = last_blink.elapsed().map_or_else(|_| blink_duration, |e| e);
            let mut rfds = FdSet::new();
            rfds.insert(pty_fd);
            rfds.insert(win_fd);

            let mut wfds = FdSet::new();
            if self.term.pty.need_flush() {
                wfds.insert(pty_fd);
            }

            // Something pending so let select just return otherwise events
            // might be delayed.
            let mut timeout_in = if self.win.is_pending() {
                TimeVal::milliseconds(0)
            } else if let Some(to) = blink_duration.checked_sub(blink_elapsed) {
                TimeVal::milliseconds(to.as_millis() as i64)
            } else {
                TimeVal::milliseconds(blink_duration.as_millis() as i64)
            };
            let timeout = Some(&mut timeout_in);
            match select(None, Some(&mut rfds), Some(&mut wfds), None, timeout) {
                Ok(_) => (),
                Err(Errno::EINTR) => continue,
                Err(err) => return Err(err.into()),
            }

            if wfds.contains(pty_fd) {
                self.term.pty.flush()?;
            }

            self.win.undraw_cursor(&self.term);

            // Let pending do it's thing so always try to process events.
            self.win.process_input(&mut self.term);

            if rfds.contains(pty_fd) {
                let n = self.term.pty.read(&mut buf)?;
                self.log_pty(&buf[..n])?;
                self.vte
                    .process_input(&buf[..n], &mut self.win, &mut self.term);
            }
            if blink_elapsed >= blink_duration {
                self.win.toggle_blink();
                last_blink = SystemTime::now();
            }
            self.win.draw(&mut self.term);
        }

        Ok(())
    }

    fn log_pty(&mut self, data: &[u8]) -> Result<()> {
        if let Some(f) = &mut self.log {
            f.write_all(data)?;
        }
        Ok(())
    }
}
