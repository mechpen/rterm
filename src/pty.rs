use crate::shell::exec_shell;
use crate::Result;
use nix::errno::Errno;
use nix::ioctl_write_ptr_bad;
use nix::libc;
use nix::pty::{forkpty, ForkptyResult};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{read, write, ForkResult, Pid};
use std::collections::VecDeque;
use std::convert::TryFrom;
use std::os::unix::io::RawFd;

ioctl_write_ptr_bad!(resizepty, libc::TIOCSWINSZ, libc::winsize);

pub struct Pty {
    master_fd: RawFd,
    child_pid: Pid,
    write_buf: VecDeque<u8>,
}

impl Pty {
    pub fn new() -> Result<Self> {
        let ForkptyResult {
            master,
            fork_result,
        } = unsafe { forkpty(None, None)? };
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
        let cols = u16::try_from(cols).unwrap();
        let rows = u16::try_from(rows).unwrap();
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
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
        let mut n = write(self.master_fd, self.write_buf.make_contiguous())?;
        if n == self.write_buf.len() {
            self.write_buf.clear();
        } else {
            while n > 0 {
                self.write_buf.pop_front();
                n -= 1;
            }
        }
        Ok(())
    }

    pub fn write(&mut self, buf: &[u8]) {
        for b in buf {
            self.write_buf.push_back(*b);
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = kill(self.child_pid, Signal::SIGHUP);
    }
}
