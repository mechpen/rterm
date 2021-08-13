use crate::pty::Pty;
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
/*
 * draw latency range in ms - from new content/keypress/etc until drawing.
 * within this range, st draws when content stops arriving (idle). mostly it's
 * near minlatency, but it waits longer for slow updates to avoid partial draw.
 * low minlatency will tear/flicker more, as it can "detect" idle too early.
 */
const MINLATENCY: f64 = 8.0;
const MAXLATENCY: f64 = 33.0;

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
    pty: Pty,
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
        let mut pty = Pty::new()?;
        pty.resize(cols, rows)?;
        Ok(App {
            win: Win::new(term.cols, term.rows, xoff, yoff, font)?,
            term,
            vte: Vte::new(),
            pty,
            log,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let win_fd = self.win.fd();
        let pty_fd = self.pty.fd();
        let mut buf = [0; 8192];
        let mut last_blink = SystemTime::now();
        let blink_duration = Duration::from_millis(500);
        let mut drawing = false;
        let mut trigger = SystemTime::now();
        let mut now;
        let mut timeout_idle: f64 = -1.0;

        while is_running() {
            let blink_elapsed = last_blink.elapsed().map_or_else(|_| blink_duration, |e| e);
            let mut rfds = FdSet::new();
            rfds.insert(pty_fd);
            rfds.insert(win_fd);

            let mut wfds = FdSet::new();
            if self.pty.need_flush() {
                wfds.insert(pty_fd);
            }

            // Something pending so let select just return otherwise events
            // might be delayed.
            let mut timeout_in = if self.win.is_pending() {
                TimeVal::milliseconds(0)
            } else if timeout_idle > 0.0 {
                TimeVal::nanoseconds((timeout_idle * 1e6) as i64)
            } else {
                TimeVal::milliseconds(blink_duration.as_millis() as i64)
            };
            let timeout = Some(&mut timeout_in);
            match select(None, Some(&mut rfds), Some(&mut wfds), None, timeout) {
                Ok(_) => (),
                Err(Errno::EINTR) => continue,
                Err(err) => return Err(err.into()),
            }
            now = SystemTime::now();

            if wfds.contains(pty_fd) {
                self.pty.flush()?;
            }

            self.win.undraw_cursor(&self.term);

            // Let pending do it's thing so always try to process events.
            let mut check_idle = self.win.process_input(&mut self.term, &mut self.pty);

            if rfds.contains(pty_fd) {
                check_idle = true;
                let n = self.pty.read(&mut buf)?;
                self.log_pty(&buf[..n])?;
                self.vte
                    .process_input(&buf[..n], &mut self.win, &mut self.term, &mut self.pty);
            }
            if blink_elapsed >= blink_duration {
                self.win.toggle_blink();
                last_blink = SystemTime::now();
            }
            /*
             * To reduce flicker and tearing, when new content or event
             * triggers drawing, we first wait a bit to ensure we got
             * everything, and if nothing new arrives - we draw.
             * We start with trying to wait minlatency ms. If more content
             * arrives sooner, we retry with shorter and shorter periods,
             * and eventually draw even without idle after maxlatency ms.
             * Typically this results in low latency while interacting,
             * maximum latency intervals during `cat huge.txt`, and perfect
             * sync with periodic updates from animations/key-repeats/etc.
             */
            if check_idle {
                if !drawing {
                    trigger = now;
                    drawing = true;
                }
                if let Ok(tdiff) = now.duration_since(trigger) {
                    timeout_idle =
                        ((MAXLATENCY - tdiff.as_millis() as f64) / MAXLATENCY) * MINLATENCY;
                    //println!("XXXX idle: {}", timeout_idle);
                    if timeout_idle > 0.0 {
                        continue; /* we have time, try to find idle */
                    }
                }
            }
            timeout_idle = -1.0;
            self.win.draw(&mut self.term);
            drawing = false;
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
