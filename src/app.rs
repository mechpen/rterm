use crate::pty::Pty;
use crate::term::Term;
use crate::utils::{parse_geometry, epoch_ms};
use crate::vte::Vte;
use crate::win::{Win, next_blink_timeout};
use crate::Result;
use nix;
use nix::errno::Errno;
use nix::sys::select::{select, FdSet};
use nix::sys::signal::{signal, SigHandler, Signal};
use nix::sys::time::{TimeVal, TimeValLike};
use std::fs::File;
use std::io::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

const MIN_DRAW_DELAY_MS: i64 = 5;
const MAX_DRAW_DELAY_MS: i64 = 50;

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
    pty: Pty,
    vte: Vte,
    log: Option<File>,
}

impl App {
    pub fn new(
        geometry: Option<&str>, font: Option<&str>, log: Option<&str>
    ) -> Result<Self> {
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
            pty: Pty::new(term.cols, term.rows)?,
            vte: Vte::new(),
            term,
            log,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let win_fd = self.win.fd();
        let pty_fd = self.pty.fd();
        let mut buf = [0; 8192];
        let mut delay_start = 0;
        let mut timeout = TimeVal::milliseconds(next_blink_timeout());

        while is_running() {
            let mut rfds = FdSet::new();
            rfds.insert(pty_fd);
            rfds.insert(win_fd);

            let mut wfds = FdSet::new();
            if self.pty.need_flush() {
                wfds.insert(pty_fd);
            }

            if self.win.pending() {
                timeout = TimeVal::milliseconds(0);
            }

            match select(
                None, Some(&mut rfds), Some(&mut wfds), None, Some(&mut timeout)
            ) {
                Ok(_) => (),
                Err(Errno::EINTR) => continue,
                Err(err) => return Err(err.into()),
            }

            if wfds.contains(pty_fd) {
                self.pty.flush()?;
            }

            // FIXME: may remove
            self.win.undraw_cursor(&self.term);

            if rfds.contains(pty_fd) {
                let n = self.pty.read(&mut buf)?;
                self.log_pty(&buf[..n])?;
                self.vte.process_input(
                    &buf[..n], &mut self.win, &mut self.term, &mut self.pty
                );
            }

            let count = self.win.process_input(&mut self.term, &mut self.pty);

            // To reduce flicker and tearing, when new content or event
            // triggers drawing, we first wait a bit to ensure we got
            // everything, and if nothing new arrives - we draw.
            // Typically this results in low latency while interacting,
            // maximum latency intervals during `cat huge.txt`, and perfect
            // sync with periodic updates from animations/key-repeats/etc.
            //
            // The equation here is simplified from the equation in st.
            if rfds.contains(pty_fd) || count > 0 {
                let now = epoch_ms();
                if delay_start == 0 {
                    delay_start = now;
                }
                if now - delay_start < MAX_DRAW_DELAY_MS {
                    timeout = TimeVal::milliseconds(MIN_DRAW_DELAY_MS);
                    continue
                }
            }

            self.win.draw(&mut self.term);
            timeout = TimeVal::milliseconds(next_blink_timeout());
            delay_start = 0;
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
