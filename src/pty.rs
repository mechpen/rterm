use nix::unistd::{
    read,
    write,
    ForkResult,
    Pid,
};
use nix::pty::{
    forkpty,
    ForkptyResult,
};
use nix::sys::signal::{
    kill,
    Signal,
};
use nix::libc;
use nix::ioctl_write_ptr_bad;
use std::os::unix::io::RawFd;
use std::convert::TryFrom;
use std::collections::VecDeque;
use crate::shell::exec_shell;
use crate::Result;

ioctl_write_ptr_bad!(resizepty, libc::TIOCSWINSZ, libc::winsize);

pub struct Pty {
    master_fd: RawFd,
    child_pid: Pid,
    write_buf: VecDeque<Vec<u8>>,
}

impl Pty {
    pub fn new() -> Result<Self> {
        let ForkptyResult { master, fork_result } = forkpty(None, None)?;
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
        unsafe { resizepty(self.master_fd, &ws)?; }
        Ok(())
    }

    pub fn fd(&self) -> RawFd {
        self.master_fd
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(read(self.master_fd, buf)?)
    }

    pub fn need_flush(&self) -> bool {
        !self.write_buf.is_empty()
    }

    pub fn flush(&mut self) -> Result<()> {
        while let Some(buf) = self.write_buf.pop_front() {
            let n = write(self.master_fd, &buf)?;
            if n == buf.len() {
                continue;
            }
            self.write_buf.push_front(buf[n..].to_vec());
        }
        Ok(())
    }

    pub fn write(&mut self, buf: Vec<u8>) {
        self.write_buf.push_back(buf);
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = kill(self.child_pid, Signal::SIGHUP);
    }
}
