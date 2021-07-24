use crate::term::Term;
use crate::utils::parse_geometry;
use crate::vte::Vte;
use crate::win::Win;
use crate::Result;
use nix;
use nix::errno::Errno;
use nix::sys::select::{select, FdSet};
use nix::sys::signal::{signal, SigHandler, Signal};
use std::fs::File;
use std::io::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

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

        while is_running() {
            let mut rfds = FdSet::new();
            rfds.insert(pty_fd);
            rfds.insert(win_fd);

            let mut wfds = FdSet::new();
            if self.term.pty.need_flush() {
                wfds.insert(pty_fd);
            }

            match select(None, Some(&mut rfds), Some(&mut wfds), None, None) {
                Ok(_) => (),
                Err(Errno::EINTR) => continue,
                Err(err) => return Err(err.into()),
            }

            if wfds.contains(pty_fd) {
                self.term.pty.flush()?;
            }

            self.win.undraw_cursor(&self.term);

            if rfds.contains(win_fd) {
                self.win.process_input(&mut self.term);
            }
            if rfds.contains(pty_fd) {
                let n = self.term.pty.read(&mut buf)?;
                self.log_pty(&buf[..n])?;
                self.vte
                    .process_input(&buf[..n], &mut self.win, &mut self.term);
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
