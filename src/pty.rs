use crate::shell::exec_shell;

use std::collections::VecDeque;
use std::convert::TryFrom;
use std::os::unix::io::RawFd;

use anyhow::Result;
use nix::errno::Errno;
use nix::ioctl_write_ptr_bad;
use nix::libc;
use nix::pty::{forkpty, ForkptyResult};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{read, write, ForkResult, Pid};

ioctl_write_ptr_bad!(resizepty, libc::TIOCSWINSZ, libc::winsize);

pub struct Pty {
    master_fd: RawFd,
    child_pid: Pid,
    write_buf: VecDeque<u8>,
}

impl Pty {
    pub fn new(cols: usize, rows: usize) -> Result<Self> {
        let ws = libc::winsize {
            ws_row: u16::try_from(rows).unwrap(),
            ws_col: u16::try_from(cols).unwrap(),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let ForkptyResult {
            master,
            fork_result,
        } = unsafe { forkpty(Some(&ws), None)? };
        let child = match fork_result {
            ForkResult::Parent { child } => child,
            ForkResult::Child => {
                exec_shell();
                unreachable!();
            }
        };

        Ok(Pty {
            master_fd: master,
            child_pid: child,
            write_buf: VecDeque::new(),
        })
    }

    pub fn resize(&mut self, cols: usize, rows: usize) -> Result<()> {
        let ws = libc::winsize {
            ws_row: u16::try_from(rows).unwrap(),
            ws_col: u16::try_from(cols).unwrap(),
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            resizepty(self.master_fd, &ws)?;
        }
        Ok(())
    }

    pub fn fd(&self) -> RawFd {
        self.master_fd
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        match read(self.master_fd, buf) {
            Ok(n) => Ok(n),
            Err(Errno::EIO) => Ok(0),
            Err(err) => Err(err.into()),
        }
    }

    pub fn need_flush(&self) -> bool {
        !self.write_buf.is_empty()
    }

    pub fn flush(&mut self) -> Result<()> {
        let (first, second) = self.write_buf.as_slices();
        let mut n = write(self.master_fd, first)?;
        if n == first.len() {
            n += write(self.master_fd, second)?;
        }
        self.write_buf.drain(..n);
        Ok(())
    }

    pub fn write(&mut self, buf: &[u8]) {
        self.write_buf.extend(buf);
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = kill(self.child_pid, Signal::SIGHUP);
    }
}
